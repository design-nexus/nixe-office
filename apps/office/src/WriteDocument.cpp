#include "WriteDocument.h"

#include <QDateTime>
#include <QAbstractTextDocumentLayout>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QFont>
#include <QJsonDocument>
#include <QJsonObject>
#include <QImage>
#include <QImageReader>
#include <QBuffer>
#include <QPdfWriter>
#include <QPainter>
#include <QPalette>
#include <QPageSize>
#include <QTextFrame>
#include <QTextFrameFormat>
#include <QLocale>
#include <QQuickTextDocument>
#include <QSaveFile>
#include <QStandardPaths>
#include <QTemporaryFile>
#include <QTextBlock>
#include <QTextBlockFormat>
#include <QTextCharFormat>
#include <QTextFragment>
#include <QTextImageFormat>
#include <QTextList>
#include <QTextListFormat>
#include <QUuid>
#include <zip.h>
#ifdef NEXUS_HAVE_ENCHANT
#include <enchant.h>
#endif

namespace {
constexpr zip_uint64_t maxEntrySize = 20 * 1024 * 1024;

QByteArray readZipEntry(zip_t *archive, const char *name) {
    zip_stat_t stat;
    zip_stat_init(&stat);
    if (zip_stat(archive, name, 0, &stat) != 0 || stat.size > maxEntrySize)
        return {};
    zip_file_t *entry = zip_fopen(archive, name, 0);
    if (!entry) return {};
    QByteArray data;
    data.resize(static_cast<qsizetype>(stat.size));
    const zip_int64_t count = zip_fread(entry, data.data(), stat.size);
    zip_fclose(entry);
    return count == static_cast<zip_int64_t>(stat.size) ? data : QByteArray();
}

bool addZipEntry(zip_t *archive, const char *name, const QByteArray &data) {
    zip_source_t *source = zip_source_buffer(archive, data.constData(), static_cast<zip_uint64_t>(data.size()), 0);
    if (!source) return false;
    if (zip_file_add(archive, name, source, ZIP_FL_ENC_UTF_8) < 0) {
        zip_source_free(source);
        return false;
    }
    return true;
}
QTextFragment imageFragmentAt(const QTextDocument &document,int position){
    QTextBlock block=document.findBlock(qBound(0,position,document.characterCount()-1));
    for(int pass=0;pass<2&&block.isValid();++pass,block=block.previous())
        for(auto it=block.begin();!it.atEnd();++it){
            const QTextFragment fragment=it.fragment();
            if(fragment.isValid()&&fragment.charFormat().isImageFormat()&&
               position>=fragment.position()&&position<=fragment.position()+fragment.length())return fragment;
        }
    return {};
}
}

WriteDocument::WriteDocument(QObject *parent) : QObject(parent) {
    QFont font(QStringLiteral("Noto Sans"));
    font.setPointSize(11);
    m_document.setDefaultFont(font);
    applyPageLayout();
    m_document.setUndoRedoEnabled(true);
    m_document.setModified(false);

    connect(&m_document, &QTextDocument::modificationChanged, this, [this](bool) {
        if (dirty() && !m_recoveryTimer.isActive()) m_recoveryTimer.start();
        if (!dirty()) m_recoveryTimer.stop();
        emit stateChanged();
    });
    connect(m_document.documentLayout(), &QAbstractTextDocumentLayout::documentSizeChanged,
            this, [this] { emit stateChanged(); });
    m_recoveryTimer.setInterval(15000);
    connect(&m_recoveryTimer, &QTimer::timeout, this, &WriteDocument::writeRecoveryNow);
    m_sessionRecovery = recoveryDirectory() + QLatin1Char('/') + QUuid::createUuid().toString(QUuid::WithoutBraces) + QStringLiteral(".json");
    findRecovery();
}

WriteDocument::~WriteDocument() {
#ifdef NEXUS_HAVE_ENCHANT
    if (m_spellDict) enchant_broker_free_dict(m_spellBroker, m_spellDict);
    if (m_spellBroker) enchant_broker_free(m_spellBroker);
#endif
}

QString WriteDocument::displayName() const {
    return m_path.isEmpty() ? QStringLiteral("Untitled") : QFileInfo(m_path).fileName();
}

void WriteDocument::setError(const QString &message) {
    if (m_error == message) return;
    m_error = message;
    emit errorChanged();
}

void WriteDocument::attachEditor(QObject *textDocument) {
    auto *quickDocument = qobject_cast<QQuickTextDocument *>(textDocument);
    if (quickDocument) quickDocument->setTextDocument(&m_document);
}

void WriteDocument::applyPageLayout() {
    m_document.setDocumentMargin(0);
    QTextFrameFormat frame=m_document.rootFrame()->frameFormat();
    frame.setTopMargin(m_topMargin);
    frame.setBottomMargin(m_bottomMargin);
    frame.setLeftMargin(m_leftMargin);
    frame.setRightMargin(m_rightMargin);
    m_document.rootFrame()->setFrameFormat(frame);
    m_document.setPageSize(QSizeF(720,930));
}

QJsonObject WriteDocument::pageSettings() const {
    return {{"top",m_topMargin},{"bottom",m_bottomMargin},{"left",m_leftMargin},{"right",m_rightMargin},
            {"header",m_headerText},{"footer",m_footerText},{"pageNumbers",m_showPageNumbers}};
}

bool WriteDocument::loadPageSettings(const QJsonObject &object) {
    const auto margin=[&](const char *key) {return object.value(QLatin1String(key)).toInt(54);};
    const int top=margin("top"),bottom=margin("bottom"),left=margin("left"),right=margin("right");
    if(top<36||top>144||bottom<36||bottom>144||left<36||left>144||right<36||right>144||
       !object.value("header").isString()&&!object.value("header").isUndefined()||
       !object.value("footer").isString()&&!object.value("footer").isUndefined())return false;
    m_topMargin=top;m_bottomMargin=bottom;m_leftMargin=left;m_rightMargin=right;
    m_headerText=object.value("header").toString().left(120);
    m_footerText=object.value("footer").toString().left(120);
    m_showPageNumbers=object.value("pageNumbers").toBool();
    return true;
}

void WriteDocument::setPageLayout(int top,int bottom,int left,int right,const QString &header,const QString &footer,bool pageNumbers) {
    if(top<36||top>144||bottom<36||bottom>144||left<36||left>144||right<36||right>144){setError("Margins must be between 36 and 144 pixels.");return;}
    const QString cleanHeader=header.left(120),cleanFooter=footer.left(120);
    if(top==m_topMargin&&bottom==m_bottomMargin&&left==m_leftMargin&&right==m_rightMargin&&
       cleanHeader==m_headerText&&cleanFooter==m_footerText&&pageNumbers==m_showPageNumbers)return;
    m_topMargin=top;m_bottomMargin=bottom;m_leftMargin=left;m_rightMargin=right;
    m_headerText=cleanHeader;m_footerText=cleanFooter;m_showPageNumbers=pageNumbers;
    applyPageLayout();m_pageDirty=true;m_document.setModified(true);
    reflowAlignedImages();
    if(!m_recoveryTimer.isActive())m_recoveryTimer.start();
    setError({});emit stateChanged();
}

void WriteDocument::newDocument() {
    m_document.clear();
    m_images.clear();
    m_topMargin=m_bottomMargin=m_leftMargin=m_rightMargin=54;
    m_headerText.clear();m_footerText.clear();m_showPageNumbers=false;
    applyPageLayout();
    m_document.setModified(false);
    m_pageDirty=false;
    m_path.clear();
    m_hasDocument = true;
    removeSessionRecovery();
    setError({});
    emit stateChanged();
}

bool WriteDocument::open(const QUrl &url) {
    const QString path = url.isLocalFile() ? url.toLocalFile() : QString();
    if (path.isEmpty()) { setError(QStringLiteral("Choose a local .nwrite file.")); return false; }
    int errorCode = 0;
    zip_t *archive = zip_open(QFile::encodeName(path).constData(), ZIP_RDONLY, &errorCode);
    if (!archive) { setError(QStringLiteral("Could not open the Nexus Write file.")); return false; }
    const QByteArray manifestData = readZipEntry(archive, "manifest.json");
    const QByteArray htmlData = readZipEntry(archive, "document.html");
    QMap<QString, QByteArray> images;
    const zip_int64_t entryCount = zip_get_num_entries(archive, 0);
    for (zip_int64_t index = 0; index < entryCount; ++index) {
        const char *entryName = zip_get_name(archive, index, 0);
        if (!entryName) continue;
        const QString name = QString::fromUtf8(entryName);
        if (!name.startsWith(QStringLiteral("media/")) || !name.endsWith(QStringLiteral(".png"))) continue;
        const QByteArray data = readZipEntry(archive, entryName);
        if (data.isEmpty() || QImage::fromData(data, "PNG").isNull()) {
            zip_close(archive);
            setError(QStringLiteral("This document contains an invalid image."));
            return false;
        }
        images.insert(name, data);
    }
    zip_close(archive);
    const QJsonDocument manifest = QJsonDocument::fromJson(manifestData);
    const QJsonObject object = manifest.object();
    if (object.value(QStringLiteral("format")) != QStringLiteral("nexus-write") ||
        (object.value(QStringLiteral("version")).toInt() != 1 && object.value(QStringLiteral("version")).toInt() != 2) || htmlData.isEmpty()) {
        setError(QStringLiteral("This file is not a supported Nexus Write document."));
        return false;
    }
    if (!loadPageSettings(object.value("page").toObject())) {
        setError(QStringLiteral("This document has invalid page settings."));return false;
    }
    m_document.clear();
    m_images = images;
    for (auto it = m_images.cbegin(); it != m_images.cend(); ++it)
        m_document.addResource(QTextDocument::ImageResource, QUrl(it.key()), QImage::fromData(it.value(), "PNG"));
    m_document.setHtml(QString::fromUtf8(htmlData));
    applyPageLayout();
    reflowAlignedImages();
    m_document.setModified(false);
    m_pageDirty=false;
    m_path = QFileInfo(path).absoluteFilePath();
    m_hasDocument = true;
    removeSessionRecovery();
    setError({});
    emit stateChanged();
    return true;
}

bool WriteDocument::save() {
    if (m_path.isEmpty()) { setError(QStringLiteral("Choose a file name to save this document.")); return false; }
    return saveAs(QUrl::fromLocalFile(m_path));
}

bool WriteDocument::saveAs(const QUrl &url) {
    QString path = url.isLocalFile() ? url.toLocalFile() : QString();
    if (path.isEmpty()) { setError(QStringLiteral("Choose a local save location.")); return false; }
    if (!path.endsWith(QStringLiteral(".nwrite"), Qt::CaseInsensitive)) path += QStringLiteral(".nwrite");

    QTemporaryFile temporary(QDir::tempPath() + QStringLiteral("/nexus-write-XXXXXX"));
    if (!temporary.open()) { setError(QStringLiteral("Could not prepare a temporary document file.")); return false; }
    const QString tempPath = temporary.fileName();
    temporary.close();
    int errorCode = 0;
    zip_t *archive = zip_open(QFile::encodeName(tempPath).constData(), ZIP_CREATE | ZIP_TRUNCATE, &errorCode);
    if (!archive) { setError(QStringLiteral("Could not create the document archive.")); return false; }
    const QByteArray manifest = QJsonDocument(QJsonObject{{QStringLiteral("format"), QStringLiteral("nexus-write")},
                                                       {QStringLiteral("version"), 2},
                                                       {QStringLiteral("content"), QStringLiteral("document.html")},
                                                       {QStringLiteral("page"), pageSettings()}}).toJson(QJsonDocument::Compact);
    const QByteArray html = m_document.toHtml().toUtf8();
    bool packaged = addZipEntry(archive, "manifest.json", manifest) && addZipEntry(archive, "document.html", html);
    for (auto it = m_images.cbegin(); packaged && it != m_images.cend(); ++it)
        packaged = addZipEntry(archive, it.key().toUtf8().constData(), it.value());
    if (!packaged) {
        zip_discard(archive);
        setError(QStringLiteral("Could not package the document."));
        return false;
    }
    if (zip_close(archive) != 0) {
        zip_discard(archive);
        setError(QStringLiteral("Could not finish the document archive."));
        return false;
    }
    QFile source(tempPath);
    if (!source.open(QIODevice::ReadOnly)) { setError(QStringLiteral("Could not read the temporary document.")); return false; }
    const QByteArray archiveData = source.readAll();
    QSaveFile destination(path);
    if (!destination.open(QIODevice::WriteOnly) || destination.write(archiveData) != archiveData.size() || !destination.commit()) {
        setError(QStringLiteral("Could not save the document. Check the destination and try again."));
        return false;
    }
    m_path = QFileInfo(path).absoluteFilePath();
    m_document.setModified(false);
    m_pageDirty=false;
    removeSessionRecovery();
    setError({});
    emit stateChanged();
    emit saved(m_path);
    return true;
}

QTextCursor WriteDocument::cursorForRange(int start, int end) {
    QTextCursor cursor(&m_document);
    const int maximum = qMax(0, m_document.characterCount() - 1);
    cursor.setPosition(qBound(0, start, maximum));
    cursor.setPosition(qBound(0, end, maximum), QTextCursor::KeepAnchor);
    return cursor;
}

void WriteDocument::toggleBold(int start, int end) {
    QTextCursor cursor = cursorForRange(start, end);
    if (!cursor.hasSelection()) cursor.select(QTextCursor::WordUnderCursor);
    QTextCharFormat format;
    format.setFontWeight(cursor.charFormat().fontWeight() >= QFont::Bold ? QFont::Normal : QFont::Bold);
    cursor.mergeCharFormat(format);
}

void WriteDocument::toggleItalic(int start, int end) {
    QTextCursor cursor = cursorForRange(start, end);
    if (!cursor.hasSelection()) cursor.select(QTextCursor::WordUnderCursor);
    QTextCharFormat format;
    format.setFontItalic(!cursor.charFormat().fontItalic());
    cursor.mergeCharFormat(format);
}

void WriteDocument::setFontFamily(int start, int end, const QString &family) {
    if (family.isEmpty() || family.size() > 100) return;
    QTextCursor cursor = cursorForRange(start, end);
    if (!cursor.hasSelection()) cursor.select(QTextCursor::WordUnderCursor);
    QTextCharFormat format;
    format.setFontFamilies({family});
    cursor.mergeCharFormat(format);
}

void WriteDocument::setFontSize(int start, int end, int points) {
    if (points < 6 || points > 96) return;
    QTextCursor cursor = cursorForRange(start, end);
    if (!cursor.hasSelection()) cursor.select(QTextCursor::WordUnderCursor);
    QTextCharFormat format;
    format.setFontPointSize(points);
    cursor.mergeCharFormat(format);
}

void WriteDocument::setFontColor(int start, int end, const QColor &color) {
    if (!color.isValid()) return;
    QTextCursor cursor = cursorForRange(start, end);
    if (!cursor.hasSelection()) cursor.select(QTextCursor::WordUnderCursor);
    QTextCharFormat format;
    format.setForeground(color);
    cursor.mergeCharFormat(format);
}

void WriteDocument::setHeading(int position, int level) {
    if (level < 0 || level > 2) return;
    QTextCursor cursor = cursorForRange(position, position);
    cursor.select(QTextCursor::BlockUnderCursor);
    QTextBlockFormat block;
    block.setHeadingLevel(level);
    cursor.mergeBlockFormat(block);
    QTextCharFormat character;
    character.setFontPointSize(level == 1 ? 20 : level == 2 ? 16 : 11);
    character.setFontWeight(level == 0 ? QFont::Normal : QFont::Bold);
    cursor.mergeCharFormat(character);
}

void WriteDocument::toggleList(int position, bool ordered) {
    QTextCursor cursor = cursorForRange(position, position);
    QTextList *list = cursor.currentList();
    const QTextListFormat::Style style = ordered ? QTextListFormat::ListDecimal : QTextListFormat::ListDisc;
    if (list && list->format().style() == style) {
        list->remove(cursor.block());
    } else {
        QTextListFormat format;
        format.setStyle(style);
        cursor.createList(format);
    }
}

bool WriteDocument::insertImage(int position, const QUrl &url) {
    const QString path = url.isLocalFile() ? url.toLocalFile() : QString();
    if (path.isEmpty() || QFileInfo(path).size() > static_cast<qint64>(maxEntrySize)) {
        setError(QStringLiteral("Choose a local image smaller than 20 MB."));
        return false;
    }
    QImageReader reader(path);
    reader.setAutoTransform(true);
    QImage image = reader.read();
    if (image.isNull()) { setError(QStringLiteral("Could not read this image.")); return false; }
    if (image.width() > 2400 || image.height() > 2400)
        image = image.scaled(2400, 2400, Qt::KeepAspectRatio, Qt::SmoothTransformation);
    QByteArray data;
    QBuffer buffer(&data);
    buffer.open(QIODevice::WriteOnly);
    if (!image.save(&buffer, "PNG") || data.size() > static_cast<qsizetype>(maxEntrySize)) {
        setError(QStringLiteral("This image is too large for a Write document."));
        return false;
    }
    const QString name = QStringLiteral("media/") + QUuid::createUuid().toString(QUuid::WithoutBraces) + QStringLiteral(".png");
    m_images.insert(name, data);
    m_document.addResource(QTextDocument::ImageResource, QUrl(name), image);
    QTextImageFormat format;
    format.setName(name);
    const qreal scale = qMin<qreal>(1.0, 610.0 / image.width());
    format.setWidth(image.width() * scale);
    format.setHeight(image.height() * scale);
    QTextCursor cursor = cursorForRange(position, position);
    if(cursor.position()!=cursor.block().position())cursor.insertBlock();
    cursor.insertImage(format);
    cursor.insertBlock();
    setError({});
    return true;
}

QVariantMap WriteDocument::imageAt(int position) const {
    const QTextFragment fragment=imageFragmentAt(m_document,position);
    if(!fragment.isValid())return {{"found",false}};
    const QTextImageFormat image=fragment.charFormat().toImageFormat();
    const Qt::Alignment alignment=m_document.findBlock(fragment.position()).blockFormat().alignment();
    const QString side=alignment.testFlag(Qt::AlignHCenter)?"center":alignment.testFlag(Qt::AlignRight)?"right":"left";
    return {{"found",true},{"position",fragment.position()},{"width",qRound(image.width())},
            {"height",qRound(image.height())},{"alignment",side}};
}

bool WriteDocument::setImageWidth(int position,int width){
    const QTextFragment fragment=imageFragmentAt(m_document,position);
    if(!fragment.isValid())return false;
    const QTextImageFormat original=fragment.charFormat().toImageFormat();
    QImage image=m_document.resource(QTextDocument::ImageResource,QUrl(original.name())).value<QImage>();
    if(image.isNull())return false;
    width=qBound(48,width,610);
    const qreal ratio=original.width()>0&&original.height()>0?original.height()/original.width():qreal(image.height())/image.width();
    QTextImageFormat updated=original;updated.setWidth(width);updated.setHeight(qMax(1,qRound(width*ratio)));
    QTextCursor cursor(&m_document);cursor.setPosition(fragment.position());
    cursor.setPosition(fragment.position()+fragment.length(),QTextCursor::KeepAnchor);
    cursor.setCharFormat(updated);
    const QString alignment=imageAt(position).value("alignment").toString();
    if(alignment=="center"||alignment=="right")setImageAlignment(position,alignment);
    setError({});return true;
}

bool WriteDocument::setImageAlignment(int position,const QString &alignment){
    const QTextFragment fragment=imageFragmentAt(m_document,position);
    if(!fragment.isValid())return false;
    Qt::Alignment side;
    if(alignment=="left")side=Qt::AlignLeft;
    else if(alignment=="center")side=Qt::AlignHCenter;
    else if(alignment=="right")side=Qt::AlignRight;
    else return false;
    QTextCursor cursor(&m_document);cursor.setPosition(fragment.position());
    QTextBlockFormat format=cursor.blockFormat();format.setAlignment(side);
    const qreal imageWidth=fragment.charFormat().toImageFormat().width();
    const qreal available=720-m_leftMargin-m_rightMargin;
    const qreal remaining=qMax<qreal>(0,available-imageWidth);
    format.setLeftMargin(alignment=="center"?remaining/2:alignment=="right"?remaining:0);
    cursor.setBlockFormat(format);setError({});return true;
}

void WriteDocument::reflowAlignedImages(){
    QVector<QPair<int,QString>> placements;
    for(QTextBlock block=m_document.begin();block.isValid();block=block.next()){
        const Qt::Alignment alignment=block.blockFormat().alignment();
        const QString side=alignment.testFlag(Qt::AlignHCenter)?"center":alignment.testFlag(Qt::AlignRight)?"right":QString();
        if(side.isEmpty())continue;
        for(auto it=block.begin();!it.atEnd();++it){
            const QTextFragment fragment=it.fragment();
            if(fragment.isValid()&&fragment.charFormat().isImageFormat())placements.append({fragment.position(),side});
        }
    }
    for(const auto &placement:placements)setImageAlignment(placement.first,placement.second);
}

void WriteDocument::insertTable(int position, int rows, int columns) {
    if (rows < 1 || rows > 20 || columns < 1 || columns > 20) return;
    QTextCursor cursor = cursorForRange(position, position);
    QTextTableFormat format;
    format.setBorder(1);
    format.setCellPadding(6);
    format.setCellSpacing(0);
    format.setWidth(QTextLength(QTextLength::PercentageLength, 100));
    cursor.insertTable(rows, columns, format);
}

bool WriteDocument::exportPdf(const QUrl &url) {
    QString path = url.isLocalFile() ? url.toLocalFile() : QString();
    if (path.isEmpty()) { setError(QStringLiteral("Choose a local PDF destination.")); return false; }
    if (!path.endsWith(QStringLiteral(".pdf"), Qt::CaseInsensitive)) path += QStringLiteral(".pdf");
    QSaveFile destination(path);
    if (!destination.open(QIODevice::WriteOnly)) {
        setError(QStringLiteral("Could not create the PDF. Check the destination."));
        return false;
    }
    {
        QPdfWriter writer(&destination);
        writer.setPageSize(QPageSize(QPageSize::Letter));
        writer.setResolution(96);
        writer.setPageMargins(QMarginsF(0,0,0,0));
        QPainter painter(&writer);
        if (!painter.isActive()) {setError(QStringLiteral("Could not render the PDF."));return false;}
        const int pages=pageCount();
        for(int page=0;page<pages;++page){
            if(page>0)writer.newPage();
            painter.save();
            painter.setClipRect(QRectF(0,0,720,930));
            painter.translate(0,-page*930);
            QAbstractTextDocumentLayout::PaintContext context;
            context.clip=QRectF(0,page*930,720,930);
            context.palette.setColor(QPalette::Text,QColor(QStringLiteral("#202329")));
            m_document.documentLayout()->draw(&painter,context);
            painter.restore();
            painter.setPen(QColor(QStringLiteral("#536071")));
            QFont font(QStringLiteral("Noto Sans"));font.setPointSize(9);painter.setFont(font);
            if(!m_headerText.isEmpty())painter.drawText(QRectF(m_leftMargin,12,720-m_leftMargin-m_rightMargin,m_topMargin-18),
                                                        Qt::AlignLeft|Qt::AlignVCenter,m_headerText);
            if(!m_footerText.isEmpty())painter.drawText(QRectF(m_leftMargin,930-m_bottomMargin+6,720-m_leftMargin-m_rightMargin,m_bottomMargin-12),
                                                        Qt::AlignLeft|Qt::AlignVCenter,m_footerText);
            if(m_showPageNumbers)painter.drawText(QRectF(m_leftMargin,930-m_bottomMargin+6,720-m_leftMargin-m_rightMargin,m_bottomMargin-12),
                                                   Qt::AlignRight|Qt::AlignVCenter,QString::number(page+1));
        }
        painter.end();
    }
    if (!destination.commit()) { setError(QStringLiteral("Could not finish the PDF.")); return false; }
    setError({});
    return true;
}

QVariantMap WriteDocument::spellingAt(int position) {
    QVariantMap result;
    QTextCursor cursor(&m_document);
    cursor.setPosition(qBound(0, position, qMax(0, m_document.characterCount() - 1)));
    cursor.select(QTextCursor::WordUnderCursor);
    const QString word = cursor.selectedText();
    result.insert(QStringLiteral("word"), word);
    result.insert(QStringLiteral("correct"), true);
    QStringList suggestions;
#ifdef NEXUS_HAVE_ENCHANT
    if (!m_spellBroker) {
        m_spellBroker = enchant_broker_init();
        if (m_spellBroker) {
            const QByteArray locale = QLocale::system().name().toUtf8();
            m_spellDict = enchant_broker_request_dict(m_spellBroker, locale.constData());
            if (!m_spellDict) m_spellDict = enchant_broker_request_dict(m_spellBroker, "en_US");
        }
    }
    if (m_spellDict && !word.isEmpty()) {
        const QByteArray utf8 = word.toUtf8();
        const int check = enchant_dict_check(m_spellDict, utf8.constData(), utf8.size());
        result.insert(QStringLiteral("correct"), check <= 0);
        if (check > 0) {
            size_t count = 0;
            char **found = enchant_dict_suggest(m_spellDict, utf8.constData(), utf8.size(), &count);
            if (found) {
                for (size_t i = 0; i < qMin<size_t>(count, 8); ++i)
                    suggestions.append(QString::fromUtf8(found[i]));
                enchant_dict_free_string_list(m_spellDict, found);
            }
        }
    }
#endif
    result.insert(QStringLiteral("available"), m_spellDict != nullptr);
    result.insert(QStringLiteral("suggestions"), suggestions);
    return result;
}

void WriteDocument::replaceWordAt(int position, const QString &replacement) {
    if (replacement.isEmpty()) return;
    QTextCursor cursor = cursorForRange(position, position);
    cursor.select(QTextCursor::WordUnderCursor);
    if (cursor.hasSelection()) cursor.insertText(replacement);
}

void WriteDocument::undo() { m_document.undo(); }
void WriteDocument::redo() { m_document.redo(); }

QString WriteDocument::recoveryDirectory() const {
    return QStandardPaths::writableLocation(QStandardPaths::AppDataLocation) + QStringLiteral("/recovery/write");
}

void WriteDocument::findRecovery() {
    QDir directory(recoveryDirectory());
    const QFileInfoList files = directory.entryInfoList({QStringLiteral("*.json")}, QDir::Files, QDir::Time);
    const QString next = files.isEmpty() ? QString() : files.first().absoluteFilePath();
    if (next != m_availableRecovery) {
        m_availableRecovery = next;
        emit recoveryAvailableChanged();
    }
}

void WriteDocument::writeRecoveryNow() {
    if (!dirty()) return;
    if (!QDir().mkpath(recoveryDirectory())) return;
    QJsonObject imageData;
    for (auto it = m_images.cbegin(); it != m_images.cend(); ++it)
        imageData.insert(it.key(), QString::fromLatin1(it.value().toBase64()));
    const QJsonObject snapshot{{QStringLiteral("format"), QStringLiteral("nexus-write-recovery")},
                               {QStringLiteral("version"), 2},
                               {QStringLiteral("path"), m_path},
                               {QStringLiteral("html"), m_document.toHtml()},
                               {QStringLiteral("page"), pageSettings()},
                               {QStringLiteral("images"), imageData},
                               {QStringLiteral("savedAt"), QDateTime::currentDateTimeUtc().toString(Qt::ISODate)}};
    QSaveFile file(m_sessionRecovery);
    if (file.open(QIODevice::WriteOnly)) {
        file.write(QJsonDocument(snapshot).toJson(QJsonDocument::Compact));
        file.commit();
    }
}

bool WriteDocument::restoreRecovery() {
    if (m_availableRecovery.isEmpty()) return false;
    QFile file(m_availableRecovery);
    if (!file.open(QIODevice::ReadOnly)) return false;
    const QJsonObject snapshot = QJsonDocument::fromJson(file.readAll()).object();
    if (snapshot.value(QStringLiteral("format")) != QStringLiteral("nexus-write-recovery") ||
        (snapshot.value(QStringLiteral("version")).toInt() != 1 && snapshot.value(QStringLiteral("version")).toInt() != 2)) return false;
    if(!loadPageSettings(snapshot.value("page").toObject()))return false;
    const QString previous = m_availableRecovery;
    m_images.clear();
    const QJsonObject imageData = snapshot.value(QStringLiteral("images")).toObject();
    for (auto it = imageData.begin(); it != imageData.end(); ++it) {
        const QByteArray data = QByteArray::fromBase64(it.value().toString().toLatin1());
        if (it.key().startsWith(QStringLiteral("media/")) && !QImage::fromData(data, "PNG").isNull())
            m_images.insert(it.key(), data);
    }
    m_document.clear();
    for (auto it = m_images.cbegin(); it != m_images.cend(); ++it)
        m_document.addResource(QTextDocument::ImageResource, QUrl(it.key()), QImage::fromData(it.value(), "PNG"));
    m_document.setHtml(snapshot.value(QStringLiteral("html")).toString());
    applyPageLayout();
    m_document.setModified(true);
    m_pageDirty=true;
    m_path = snapshot.value(QStringLiteral("path")).toString();
    m_hasDocument = true;
    m_availableRecovery.clear();
    QFile::remove(previous);
    writeRecoveryNow();
    emit recoveryAvailableChanged();
    emit stateChanged();
    return true;
}

void WriteDocument::removeSessionRecovery() { QFile::remove(m_sessionRecovery); }

void WriteDocument::discardRecovery() {
    removeSessionRecovery();
    if (!m_availableRecovery.isEmpty()) {
        QFile::remove(m_availableRecovery);
        m_availableRecovery.clear();
        emit recoveryAvailableChanged();
    }
}

void WriteDocument::discardSessionRecovery() { removeSessionRecovery(); }
