#include "PresentDocument.h"

#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QImageReader>
#include <QBuffer>
#include <QSet>
#include <QPageSize>
#include <QPainter>
#include <QPdfWriter>
#include <QSaveFile>
#include <QJsonDocument>
#include <QUuid>
#include <utility>

PresentDocument::PresentDocument(QObject *parent) : QAbstractListModel(parent) {
    m_recoveryTimer.setInterval(15000);
    connect(&m_recoveryTimer, &QTimer::timeout, this, &PresentDocument::writeRecoveryNow);
    m_sessionRecovery = recoveryDirectory() + QLatin1Char('/') + QUuid::createUuid().toString(QUuid::WithoutBraces) + QStringLiteral(".json");
    findRecovery();
}

int PresentDocument::rowCount(const QModelIndex &parent) const {
    return parent.isValid() ? 0 : m_slides.size();
}

QHash<int, QByteArray> PresentDocument::roleNames() const {
    return {{Qt::DisplayRole, "title"}, {Qt::UserRole + 1, "body"},
            {Qt::UserRole + 2, "layout"}, {Qt::UserRole + 3, "style"},
            {Qt::UserRole + 4, "slideNumber"}, {Qt::UserRole + 5, "fontFamily"},
            {Qt::UserRole + 6, "fontColor"}, {Qt::UserRole + 7, "backgroundColor"},
            {Qt::UserRole + 8, "bold"}, {Qt::UserRole + 9, "italic"},
            {Qt::UserRole + 10, "elements"}};
}

QVariant PresentDocument::data(const QModelIndex &index, int role) const {
    if (!index.isValid() || index.row() < 0 || index.row() >= m_slides.size()) return {};
    const Slide &slide = m_slides.at(index.row());
    if (role == Qt::DisplayRole) return slide.title;
    if (role == Qt::UserRole + 1) return slide.body;
    if (role == Qt::UserRole + 2) return slide.layout;
    if (role == Qt::UserRole + 3) return slide.style;
    if (role == Qt::UserRole + 4) return index.row() + 1;
    if (role == Qt::UserRole + 5) return slide.fontFamily;
    if (role == Qt::UserRole + 6) return slide.fontColor;
    if (role == Qt::UserRole + 7) return slide.backgroundColor;
    if (role == Qt::UserRole + 8) return slide.bold;
    if (role == Qt::UserRole + 9) return slide.italic;
    if (role == Qt::UserRole + 10) return elementsFor(slide);
    return {};
}

QString PresentDocument::displayName() const {
    return m_path.isEmpty() ? QStringLiteral("Untitled presentation") : QFileInfo(m_path).fileName();
}

PresentDocument::Slide *PresentDocument::current() {
    return m_selected >= 0 && m_selected < m_slides.size() ? &m_slides[m_selected] : nullptr;
}
const PresentDocument::Slide *PresentDocument::current() const {
    return m_selected >= 0 && m_selected < m_slides.size() ? &m_slides[m_selected] : nullptr;
}
QString PresentDocument::currentTitle() const { const Slide *slide = current(); return slide ? slide->title : QString(); }
QString PresentDocument::currentBody() const { const Slide *slide = current(); return slide ? slide->body : QString(); }
QString PresentDocument::currentNotes() const { const Slide *slide = current(); return slide ? slide->notes : QString(); }
int PresentDocument::currentLayout() const { const Slide *slide = current(); return slide ? slide->layout : 0; }
int PresentDocument::currentStyle() const { const Slide *slide = current(); return slide ? slide->style : 0; }

QString PresentDocument::currentFontFamily() const { const Slide *s = current(); return s ? s->fontFamily : QStringLiteral("Noto Sans"); }
QString PresentDocument::currentFontColor() const { const Slide *s = current(); return s ? s->fontColor : QString(); }
QString PresentDocument::currentBackgroundColor() const { const Slide *s = current(); return s ? s->backgroundColor : QString(); }
bool PresentDocument::currentBold() const { const Slide *s = current(); return s && s->bold; }
bool PresentDocument::currentItalic() const { const Slide *s = current(); return s && s->italic; }
int PresentDocument::currentTitleSize() const { const Slide *s = current(); return s ? s->titleSize : 0; }
int PresentDocument::currentBodySize() const { const Slide *s = current(); return s ? s->bodySize : 0; }

QVariantMap PresentDocument::slideAt(int slideIndex) const {
    if (slideIndex < 0 || slideIndex >= m_slides.size()) return {};
    const Slide &slide = m_slides.at(slideIndex);
    return {{QStringLiteral("title"), slide.title},
            {QStringLiteral("body"), slide.body},
            {QStringLiteral("notes"), slide.notes},
            {QStringLiteral("layout"), slide.layout},
            {QStringLiteral("style"), slide.style},
            {QStringLiteral("fontFamily"), slide.fontFamily}, {QStringLiteral("fontColor"), slide.fontColor},
            {QStringLiteral("backgroundColor"), slide.backgroundColor}, {QStringLiteral("bold"), slide.bold},
            {QStringLiteral("italic"), slide.italic}, {QStringLiteral("titleSize"), slide.titleSize},
            {QStringLiteral("bodySize"), slide.bodySize}, {QStringLiteral("elements"), elementsFor(slide)}};
}

void PresentDocument::setError(const QString &error) {
    if (m_error == error) return;
    m_error = error;
    emit errorChanged();
}
void PresentDocument::changed() {
    m_dirty = true;
    m_hasDocument = true;
    if (m_selected >= 0)
        emit dataChanged(index(m_selected), index(m_selected));
    emit currentSlideChanged();
    emit stateChanged();
}

void PresentDocument::newDocument() {
    beginResetModel();
    m_slides = {Slide()};
    endResetModel();
    m_selected = 0;
    m_path.clear();
    m_dirty = false;
    m_hasDocument = true;
    clearHistory();
    m_savedState = QJsonDocument(nativeObject()).toJson(QJsonDocument::Compact);
    removeSessionRecovery();
    setError({});
    emit selectionChanged();
    emit currentSlideChanged();
    emit stateChanged();
}

void PresentDocument::selectSlide(int slideIndex) {
    if (slideIndex < 0 || slideIndex >= m_slides.size() || slideIndex == m_selected) return;
    m_selected = slideIndex;
    m_lastTextEditKey.clear();
    emit selectionChanged();
    emit currentSlideChanged();
}

void PresentDocument::addSlide() {
    if (m_slides.size() >= 200) {
        setError(QStringLiteral("A presentation can contain up to 200 slides."));
        return;
    }
    const int insertion = qMax(0, m_selected + 1);
    recordEdit();
    beginInsertRows({}, insertion, insertion);
    Slide slide;
    slide.layout = 1;
    slide.style = currentStyle();
    slide.fontFamily = currentFontFamily();
    slide.fontColor = currentFontColor();
    slide.backgroundColor = currentBackgroundColor();
    slide.bold = currentBold();
    slide.italic = currentItalic();
    slide.titleSize = currentTitleSize();
    slide.bodySize = currentBodySize();
    m_slides.insert(insertion, slide);
    endInsertRows();
    m_selected = insertion;
    setError({});
    changed();
    emit selectionChanged();
}

void PresentDocument::duplicateSlide() {
    if (!current()) return;
    if (m_slides.size() >= 200) {
        setError(QStringLiteral("A presentation can contain up to 200 slides."));
        return;
    }
    const Slide copy = *current();
    const int insertion = m_selected + 1;
    recordEdit();
    beginInsertRows({}, insertion, insertion);
    m_slides.insert(insertion, copy);
    endInsertRows();
    m_selected = insertion;
    setError({});
    changed();
    emit selectionChanged();
}

void PresentDocument::deleteSlide() {
    if (!current()) return;
    if (m_slides.size() == 1) {
        if (current()->title.isEmpty() && current()->body.isEmpty() && current()->notes.isEmpty() &&
            current()->layout == 0 && current()->style == 0 &&
            current()->fontFamily == QStringLiteral("Noto Sans") && current()->fontColor.isEmpty() &&
            current()->backgroundColor.isEmpty() && !current()->bold && !current()->italic &&
            current()->titleSize == 0 && current()->bodySize == 0 && current()->elements.isEmpty()) return;
        recordEdit();
        *current() = Slide();
        changed();
        emit selectionChanged();
        return;
    }
    const int removal = m_selected;
    recordEdit();
    beginRemoveRows({}, removal, removal);
    m_slides.removeAt(removal);
    endRemoveRows();
    m_selected = qMin(removal, m_slides.size() - 1);
    changed();
    emit selectionChanged();
}

void PresentDocument::moveSlide(int direction) {
    const int target = m_selected + direction;
    if (!current() || (direction != -1 && direction != 1) ||
        target < 0 || target >= m_slides.size()) return;
    recordEdit();
    std::swap(m_slides[m_selected], m_slides[target]);
    emit dataChanged(index(qMin(m_selected, target)), index(qMax(m_selected, target)));
    m_selected = target;
    changed();
    emit selectionChanged();
}

void PresentDocument::setTitle(const QString &title) {
    Slide *slide = current();
    if (!slide || slide->title == title) return;
    recordTextEdit(QStringLiteral("title"));
    slide->title = title;
    changed();
}
void PresentDocument::setBody(const QString &body) {
    Slide *slide = current();
    if (!slide || slide->body == body) return;
    recordTextEdit(QStringLiteral("body"));
    slide->body = body;
    changed();
}
void PresentDocument::setNotes(const QString &notes) {
    Slide *slide = current();
    if (!slide || slide->notes == notes) return;
    recordTextEdit(QStringLiteral("notes"));
    slide->notes = notes;
    changed();
}
void PresentDocument::setLayout(int layout) {
    Slide *slide = current();
    if (!slide || layout < 0 || layout > 2 || slide->layout == layout) return;
    recordEdit();
    slide->layout = layout;
    changed();
}
void PresentDocument::setStyle(int style) {
    Slide *slide = current();
    if (!slide || style < 0 || style > 2 || (slide->style == style && slide->backgroundColor.isEmpty())) return;
    recordEdit();
    slide->style = style;
    slide->backgroundColor.clear();
    changed();
}

void PresentDocument::setFontFamily(const QString &family) {
    Slide *s = current();
    if (!s || family.isEmpty() || family.size() > 100 || s->fontFamily == family) return;
    recordEdit();
    s->fontFamily = family; changed();
}
void PresentDocument::setFontColor(const QString &color) {
    Slide *s = current();
    if (!s || !QColor(color).isValid() || s->fontColor == color) return;
    recordEdit();
    s->fontColor = color; changed();
}
void PresentDocument::setBackgroundColor(const QString &color) {
    Slide *s = current();
    if (!s || !QColor(color).isValid() || s->backgroundColor == color) return;
    recordEdit();
    s->backgroundColor = color; changed();
}
void PresentDocument::setBold(bool bold) { Slide *s = current(); if (s && s->bold != bold) { recordEdit(); s->bold = bold; changed(); } }
void PresentDocument::setItalic(bool italic) { Slide *s = current(); if (s && s->italic != italic) { recordEdit(); s->italic = italic; changed(); } }
void PresentDocument::setTitleSize(int size) { Slide *s = current(); if (s && size >= 18 && size <= 120 && s->titleSize != size) { recordEdit(); s->titleSize = size; changed(); } }
void PresentDocument::setBodySize(int size) { Slide *s = current(); if (s && size >= 12 && size <= 96 && s->bodySize != size) { recordEdit(); s->bodySize = size; changed(); } }

bool PresentDocument::open(const QUrl &url) {
    const QString path = url.isLocalFile() ? url.toLocalFile() : QString();
    QFile file(path);
    if (path.isEmpty() || !file.open(QIODevice::ReadOnly) || file.size() > 20 * 1024 * 1024) {
        setError(QStringLiteral("Could not open this Nexus Present file."));
        return false;
    }
    QJsonParseError parseError;
    const QJsonDocument parsed = QJsonDocument::fromJson(file.readAll(), &parseError);
    const QJsonObject object = parsed.object();
    const QJsonArray slides = object.value(QStringLiteral("slides")).toArray();
    if (parseError.error != QJsonParseError::NoError ||
        object.value(QStringLiteral("format")) != QStringLiteral("nexus-present") ||
        (object.value(QStringLiteral("version")).toInt() != 1 && object.value(QStringLiteral("version")).toInt() != 2) ||
        slides.isEmpty() || slides.size() > 200) {
        setError(QStringLiteral("This is not a supported Nexus Present file."));
        return false;
    }
    QVector<Slide> loaded;
    loaded.reserve(slides.size());
    for (const QJsonValue &value : slides) {
        if (!value.isObject()) { setError(QStringLiteral("This presentation contains an invalid slide.")); return false; }
        const QJsonObject item = value.toObject();
        const int layout = item.value(QStringLiteral("layout")).toInt(-1);
        const int style = item.value(QStringLiteral("style")).toInt(-1);
        const QString title = item.value(QStringLiteral("title")).toString();
        const QString body = item.value(QStringLiteral("body")).toString();
        const QString notes = item.value(QStringLiteral("notes")).toString();
        if (layout < 0 || layout > 2 || style < 0 || style > 2 ||
            title.size() > 5000 || body.size() > 100000 || notes.size() > 100000 ||
            !item.value(QStringLiteral("title")).isString() ||
            !item.value(QStringLiteral("body")).isString() ||
            !item.value(QStringLiteral("notes")).isString()) {
            setError(QStringLiteral("This presentation contains an invalid slide."));
            return false;
        }
        Slide slide;
        slide.title = title; slide.body = body; slide.notes = notes; slide.layout = layout; slide.style = style;
        slide.fontFamily = item.value(QStringLiteral("fontFamily")).toString(QStringLiteral("Noto Sans"));
        slide.fontColor = item.value(QStringLiteral("fontColor")).toString();
        slide.backgroundColor = item.value(QStringLiteral("backgroundColor")).toString();
        slide.bold = item.value(QStringLiteral("bold")).toBool();
        slide.italic = item.value(QStringLiteral("italic")).toBool();
        slide.titleSize = item.value(QStringLiteral("titleSize")).toInt();
        slide.bodySize = item.value(QStringLiteral("bodySize")).toInt();
        if (slide.fontFamily.isEmpty() || slide.fontFamily.size() > 100 ||
            (!slide.fontColor.isEmpty() && !QColor(slide.fontColor).isValid()) ||
            (!slide.backgroundColor.isEmpty() && !QColor(slide.backgroundColor).isValid()) ||
            (slide.titleSize && (slide.titleSize < 18 || slide.titleSize > 120)) ||
            (slide.bodySize && (slide.bodySize < 12 || slide.bodySize > 96))) {
            setError(QStringLiteral("This presentation contains invalid formatting.")); return false;
        }
        const QJsonArray elements=item.value("elements").toArray();
        if(elements.size()>30){setError("This slide has too many objects.");return false;}
        QSet<QString> ids;
        for(const QJsonValue &value:elements){
            if(!value.isObject()){setError("This slide contains an invalid object.");return false;}
            QJsonObject obj=value.toObject();Element element;
            element.id=obj.value("id").toString();element.type=obj.value("type").toString();
            element.x=obj.value("x").toInt(-1);element.y=obj.value("y").toInt(-1);
            element.width=obj.value("width").toInt(-1);element.height=obj.value("height").toInt(-1);
            element.text=obj.value("text").toString();element.shape=obj.value("shape").toString();
            element.fill=obj.value("fill").toString();
            if(element.id.isEmpty()||ids.contains(element.id)||
               (element.type!="text"&&element.type!="image"&&element.type!="shape")||
               element.width<48||element.height<32||element.x<0||element.y<0||
               element.x+element.width>960||element.y+element.height>540||element.text.size()>10000||
               (element.type=="shape"&&((element.shape!="rectangle"&&element.shape!="ellipse")||!QColor(element.fill).isValid()))){
                setError("This slide contains an invalid object.");return false;}
            ids.insert(element.id);
            if(element.type=="image"){
                const QByteArray encoded=obj.value("image").toString().toLatin1();
                if(encoded.size()>4*1024*1024){setError("This slide contains an oversized image.");return false;}
                element.image=QByteArray::fromBase64(encoded);
                QBuffer buffer(&element.image);buffer.open(QIODevice::ReadOnly);QImageReader reader(&buffer,"PNG");
                const QSize size=reader.size();
                if(element.image.isEmpty()||!size.isValid()||size.width()>1600||size.height()>900||reader.read().isNull()){
                    setError("This slide contains an invalid image.");return false;}
            }
            slide.elements.append(element);
        }
        loaded.append(slide);
    }
    beginResetModel();
    m_slides = std::move(loaded);
    endResetModel();
    m_selected = 0;
    m_path = QFileInfo(path).absoluteFilePath();
    m_dirty = false;
    m_hasDocument = true;
    clearHistory();
    m_savedState = QJsonDocument(nativeObject()).toJson(QJsonDocument::Compact);
    removeSessionRecovery();
    setError({});
    emit selectionChanged();
    emit currentSlideChanged();
    emit stateChanged();
    return true;
}

bool PresentDocument::save() {
    if (m_path.isEmpty()) {
        setError(QStringLiteral("Choose a presentation file name first."));
        return false;
    }
    return saveAs(QUrl::fromLocalFile(m_path));
}

QJsonObject PresentDocument::nativeObject() const {
    QJsonArray slides;
    for (const Slide &slide : m_slides) {
        QJsonArray elements;
        for(const Element &element:slide.elements){
            QJsonObject item{{"id",element.id},{"type",element.type},{"x",element.x},{"y",element.y},
                             {"width",element.width},{"height",element.height},{"text",element.text},
                             {"shape",element.shape},{"fill",element.fill}};
            if(element.type=="image")item.insert("image",QString::fromLatin1(element.image.toBase64()));
            elements.append(item);
        }
        slides.append(QJsonObject{{QStringLiteral("title"), slide.title},
                                  {QStringLiteral("body"), slide.body},
                                  {QStringLiteral("notes"), slide.notes},
                                  {QStringLiteral("layout"), slide.layout},
                                  {QStringLiteral("style"), slide.style},
                                  {QStringLiteral("fontFamily"), slide.fontFamily},
                                  {QStringLiteral("fontColor"), slide.fontColor},
                                  {QStringLiteral("backgroundColor"), slide.backgroundColor},
                                  {QStringLiteral("bold"), slide.bold}, {QStringLiteral("italic"), slide.italic},
                                  {QStringLiteral("titleSize"), slide.titleSize},
                                  {QStringLiteral("bodySize"), slide.bodySize},
                                  {QStringLiteral("elements"), elements}});
    }
    return QJsonObject{{QStringLiteral("format"), QStringLiteral("nexus-present")},
                             {QStringLiteral("version"), 2},
                             {QStringLiteral("slides"), slides}};
}

bool PresentDocument::saveAs(const QUrl &url) {
    QString path = url.isLocalFile() ? url.toLocalFile() : QString();
    if (path.isEmpty()) { setError(QStringLiteral("Choose a local save destination.")); return false; }
    if (!path.endsWith(QStringLiteral(".npresent"), Qt::CaseInsensitive)) path += QStringLiteral(".npresent");
    const QByteArray data = QJsonDocument(nativeObject()).toJson(QJsonDocument::Compact);
    if (data.size() > 20 * 1024 * 1024) {
        setError(QStringLiteral("This presentation is too large to save."));
        return false;
    }
    QSaveFile file(path);
    if (!file.open(QIODevice::WriteOnly) || file.write(data) != data.size() || !file.commit()) {
        setError(QStringLiteral("Could not save the presentation."));
        return false;
    }
    m_path = QFileInfo(path).absoluteFilePath();
    m_dirty = false;
    m_savedState = data;
    m_lastTextEditKey.clear();
    removeSessionRecovery();
    setError({});
    emit stateChanged();
    return true;
}

bool PresentDocument::exportPdf(const QUrl &url, const QColor &accent) {
    if (m_slides.isEmpty()) {
        setError(QStringLiteral("Create a presentation before exporting a PDF."));
        return false;
    }
    QString path = url.isLocalFile() ? url.toLocalFile() : QString();
    if (path.isEmpty()) {
        setError(QStringLiteral("Choose a local PDF destination."));
        return false;
    }
    if (!path.endsWith(QStringLiteral(".pdf"), Qt::CaseInsensitive)) path += QStringLiteral(".pdf");
    QSaveFile file(path);
    if (!file.open(QIODevice::WriteOnly)) {
        setError(QStringLiteral("Could not create the PDF."));
        return false;
    }
    bool pageError = false;
    {
        QPdfWriter writer(&file);
        writer.setPageSize(QPageSize(QSizeF(960, 540), QPageSize::Point));
        writer.setPageMargins(QMarginsF(0, 0, 0, 0));
        writer.setResolution(72);
        QPainter painter(&writer);
        painter.setRenderHint(QPainter::Antialiasing);
        painter.setRenderHint(QPainter::TextAntialiasing);
        for (int slideIndex = 0; slideIndex < m_slides.size(); ++slideIndex) {
            if (slideIndex > 0 && !writer.newPage()) { pageError = true; break; }
            const Slide &slide = m_slides.at(slideIndex);
            const QColor background = !slide.backgroundColor.isEmpty() ? QColor(slide.backgroundColor) : slide.style == 1 ? QColor(QStringLiteral("#20263a"))
                : slide.style == 2 ? (accent.isValid() ? accent : QColor(QStringLiteral("#89b4fa")))
                                   : QColor(Qt::white);
            const QColor foreground = !slide.fontColor.isEmpty() ? QColor(slide.fontColor) : slide.style == 0 ? QColor(QStringLiteral("#202536")) : QColor(Qt::white);
            painter.fillRect(QRect(0, 0, 960, 540), background);
            painter.setPen(foreground);
            const QRect titleRect(85, slide.layout == 1 ? 76 : slide.layout == 2 ? 182 : 187,
                                  790, slide.layout == 1 ? 105 : 116);
            const QRect bodyRect(91, slide.layout == 1 ? 197 : slide.layout == 2 ? 306 : 326,
                                 778, slide.layout == 1 ? 255 : 110);
            QFont titleFont(slide.fontFamily);
            titleFont.setPixelSize(slide.titleSize ? slide.titleSize : slide.layout == 1 ? 49 : 58);
            titleFont.setWeight(slide.bold ? QFont::Bold : QFont::DemiBold);
            titleFont.setItalic(slide.italic);
            painter.setFont(titleFont);
            painter.save();
            painter.setClipRect(titleRect);
            painter.drawText(titleRect, Qt::TextWordWrap | Qt::AlignVCenter |
                             (slide.layout == 1 ? Qt::AlignLeft : Qt::AlignHCenter), slide.title);
            painter.restore();
            QFont bodyFont(slide.fontFamily);
            bodyFont.setPixelSize(slide.bodySize ? slide.bodySize : slide.layout == 1 ? 26 : 29);
            bodyFont.setBold(slide.bold);
            bodyFont.setItalic(slide.italic);
            painter.setFont(bodyFont);
            painter.save();
            painter.setClipRect(bodyRect);
            painter.drawText(bodyRect, Qt::TextWordWrap |
                             (slide.layout == 1 ? Qt::AlignTop | Qt::AlignLeft
                                                : Qt::AlignVCenter | Qt::AlignHCenter), slide.body);
            painter.restore();
            for(const Element &element:slide.elements){
                const QRect bounds(element.x,element.y,element.width,element.height);
                painter.save();painter.setClipRect(bounds);
                if(element.type=="image"){
                    QImage image=QImage::fromData(element.image,"PNG");
                    if(!image.isNull()){
                        QSize fitted=image.size().scaled(bounds.size(),Qt::KeepAspectRatio);
                        QRect target(bounds.x()+(bounds.width()-fitted.width())/2,
                                     bounds.y()+(bounds.height()-fitted.height())/2,fitted.width(),fitted.height());
                        painter.drawImage(target,image);
                    }
                }else if(element.type=="shape"){
                    painter.setPen(Qt::NoPen);painter.setBrush(QColor(element.fill));
                    if(element.shape=="ellipse")painter.drawEllipse(bounds);else painter.drawRect(bounds);
                }else{
                    QFont font(slide.fontFamily);font.setPixelSize(slide.bodySize?slide.bodySize:28);
                    font.setBold(slide.bold);font.setItalic(slide.italic);
                    painter.setFont(font);painter.setPen(foreground);
                    painter.drawText(bounds,Qt::TextWordWrap|Qt::AlignLeft|Qt::AlignVCenter,element.text);
                }
                painter.restore();
            }
        }
        painter.end();
    }
    if (pageError || !file.commit()) {
        setError(QStringLiteral("Could not finish the PDF."));
        return false;
    }
    setError({});
    return true;
}
