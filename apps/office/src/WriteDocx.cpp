#include "WriteDocument.h"
#include "OfficeZip.h"

#include <QBuffer>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QImage>
#include <QImageReader>
#include <QSaveFile>
#include <QTemporaryFile>
#include <QTextBlock>
#include <QTextBlockFormat>
#include <QTextFragment>
#include <QTextFrame>
#include <QTextImageFormat>
#include <QTextList>
#include <QTextTable>
#include <QXmlStreamReader>
#include <QXmlStreamWriter>
#include <QUuid>
#include <zip.h>

namespace {
constexpr auto wordNamespace = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
constexpr auto relationNamespace = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
constexpr auto drawingNamespace = "http://schemas.openxmlformats.org/drawingml/2006/main";
constexpr auto wordDrawingNamespace = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
constexpr auto officeRelationship = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
constexpr zip_uint64_t maxXmlSize = 20 * 1024 * 1024;

QByteArray readEntry(zip_t *archive, const QByteArray &name) {
    zip_stat_t stat;
    zip_stat_init(&stat);
    if (zip_stat(archive, name.constData(), 0, &stat) != 0 || stat.size > maxXmlSize) return {};
    zip_file_t *entry = zip_fopen(archive, name.constData(), 0);
    if (!entry) return {};
    QByteArray data(static_cast<qsizetype>(stat.size), '\0');
    const zip_int64_t count = zip_fread(entry, data.data(), stat.size);
    zip_fclose(entry);
    return count == static_cast<zip_int64_t>(stat.size) ? data : QByteArray();
}
bool addEntry(zip_t *archive, const char *name, const QByteArray &data) {
    zip_source_t *source = zip_source_buffer(archive, data.constData(), static_cast<zip_uint64_t>(data.size()), 0);
    if (!source) return false;
    if (zip_file_add(archive, name, source, ZIP_FL_ENC_UTF_8) < 0) { zip_source_free(source); return false; }
    return true;
}
QString escapedRunText(const QString &text) {
    QString result;
    for (const QChar ch : text) {
        if (ch == QLatin1Char('\n')) result += QStringLiteral("<br/>");
        else if (ch == QLatin1Char('\t')) result += QStringLiteral("&emsp;");
        else result += QString(ch).toHtmlEscaped();
    }
    return result;
}
struct ImportedRun {
    QString text;
    bool bold = false;
    bool italic = false;
    bool underline = false;
    QString family;
    QString color;
    int halfPoints = 0;
};
QString runHtml(const ImportedRun &run) {
    QString css;
    if (run.bold) css += QStringLiteral("font-weight:700;");
    if (run.italic) css += QStringLiteral("font-style:italic;");
    if (run.underline) css += QStringLiteral("text-decoration:underline;");
    if (!run.family.isEmpty()) css += QStringLiteral("font-family:'") + run.family.toHtmlEscaped() + QStringLiteral("';");
    if (!run.color.isEmpty()) css += QStringLiteral("color:#") + run.color.toHtmlEscaped() + QLatin1Char(';');
    if (run.halfPoints > 0) css += QStringLiteral("font-size:%1pt;").arg(run.halfPoints / 2.0);
    const QString content = escapedRunText(run.text);
    return css.isEmpty() ? content : QStringLiteral("<span style=\"") + css + QStringLiteral("\">") + content + QStringLiteral("</span>");
}
bool isWord(const QXmlStreamReader &reader, QLatin1StringView local) {
    return reader.namespaceUri() == QLatin1StringView(wordNamespace) && reader.name() == local;
}
QString attribute(const QXmlStreamReader &reader, QLatin1StringView local) {
    return reader.attributes().value(QLatin1StringView(wordNamespace), local).toString();
}
QString readDocumentHtml(const QByteArray &xml,const QHash<QString,QString> &imageByRelation,
                         const QMap<QString,QByteArray> &images,bool &okay) {
    QXmlStreamReader reader(xml);
    QString html = QStringLiteral("<html><body style=\"font-family:'Noto Sans';font-size:11pt;color:#202329\">");
    bool body = false, paragraph = false, properties = false, run = false, runProperties = false;
    bool numbered = false, sawParagraph = false;
    int heading = 0, tableDepth = 0;
    ImportedRun currentRun;
    QString paragraphHtml;
    QString paragraphAlignment;
    bool drawing=false,drawingAdded=false;
    qint64 drawingWidth=0,drawingHeight=0;
    while (!reader.atEnd()) {
        reader.readNext();
        if (reader.isStartElement()) {
            if (isWord(reader, QLatin1StringView("body"))) body = true;
            if (!body) continue;
            if (isWord(reader, QLatin1StringView("tbl"))) { html += QStringLiteral("<table border=\"1\" cellspacing=\"0\" cellpadding=\"6\">"); ++tableDepth; }
            else if (isWord(reader, QLatin1StringView("tr"))) html += QStringLiteral("<tr>");
            else if (isWord(reader, QLatin1StringView("tc"))) html += QStringLiteral("<td>");
            else if (isWord(reader, QLatin1StringView("p"))) { paragraph = true; properties = false; heading = 0; numbered = false; paragraphHtml.clear(); paragraphAlignment.clear(); sawParagraph = true; }
            else if (paragraph && isWord(reader, QLatin1StringView("pPr"))) properties = true;
            else if (properties && isWord(reader, QLatin1StringView("pStyle"))) {
                const QString style = attribute(reader, QLatin1StringView("val"));
                if (style == QStringLiteral("Heading1") || style == QStringLiteral("Title")) heading = 1;
                else if (style == QStringLiteral("Heading2")) heading = 2;
            } else if (properties && isWord(reader, QLatin1StringView("numPr"))) numbered = true;
            else if(properties&&isWord(reader,QLatin1StringView("jc"))){
                const QString value=attribute(reader,QLatin1StringView("val"));
                if(value=="center"||value=="right")paragraphAlignment=value;
            }
            else if (paragraph && isWord(reader, QLatin1StringView("r"))) { run = true; currentRun = ImportedRun(); }
            else if (run && isWord(reader, QLatin1StringView("rPr"))) runProperties = true;
            else if (runProperties && isWord(reader, QLatin1StringView("b"))) currentRun.bold = attribute(reader, QLatin1StringView("val")) != QStringLiteral("0");
            else if (runProperties && isWord(reader, QLatin1StringView("i"))) currentRun.italic = attribute(reader, QLatin1StringView("val")) != QStringLiteral("0");
            else if (runProperties && isWord(reader, QLatin1StringView("u"))) currentRun.underline = attribute(reader, QLatin1StringView("val")) != QStringLiteral("none");
            else if (runProperties && isWord(reader, QLatin1StringView("color"))) {
                const QString value = attribute(reader, QLatin1StringView("val"));
                if (value.size() == 6) currentRun.color = value;
            } else if (runProperties && isWord(reader, QLatin1StringView("rFonts"))) currentRun.family = attribute(reader, QLatin1StringView("ascii"));
            else if (runProperties && isWord(reader, QLatin1StringView("sz"))) currentRun.halfPoints = attribute(reader, QLatin1StringView("val")).toInt();
            else if (run && isWord(reader, QLatin1StringView("t"))) currentRun.text += reader.readElementText(QXmlStreamReader::IncludeChildElements);
            else if (run && isWord(reader, QLatin1StringView("tab"))) currentRun.text += QLatin1Char('\t');
            else if (run && isWord(reader, QLatin1StringView("br"))) currentRun.text += QLatin1Char('\n');
            else if(paragraph&&isWord(reader,QLatin1StringView("drawing"))){
                if(run){paragraphHtml+=runHtml(currentRun);currentRun.text.clear();}
                drawing=true;drawingAdded=false;drawingWidth=drawingHeight=0;
            }else if(drawing&&reader.namespaceUri()==QLatin1StringView(wordDrawingNamespace)&&reader.name()==QLatin1StringView("extent")){
                drawingWidth=reader.attributes().value("cx").toLongLong();
                drawingHeight=reader.attributes().value("cy").toLongLong();
            }else if(drawing&&reader.namespaceUri()==QLatin1StringView(drawingNamespace)&&reader.name()==QLatin1StringView("blip")&&!drawingAdded){
                const QString relation=reader.attributes().value(QLatin1StringView(relationNamespace),QLatin1StringView("embed")).toString();
                const QString name=imageByRelation.value(relation);
                if(!name.isEmpty()&&images.contains(name)){
                    const QImage image=QImage::fromData(images.value(name),"PNG");
                    qreal width=drawingWidth>0?drawingWidth/9525.0:image.width();
                    qreal height=drawingHeight>0?drawingHeight/9525.0:image.height();
                    if(width<=0||height<=0){width=image.width();height=image.height();}
                    const qreal scale=qMin<qreal>(1.0,610.0/width);
                    paragraphHtml+=QStringLiteral("<img src=\"%1\" width=\"%2\" height=\"%3\"/>")
                        .arg(name.toHtmlEscaped()).arg(qMax(1,qRound(width*scale))).arg(qMax(1,qRound(height*scale)));
                    drawingAdded=true;
                }
            }else if(paragraph&&isWord(reader,QLatin1StringView("pict")))paragraphHtml+=QStringLiteral("[Image omitted]");
        } else if (reader.isEndElement()) {
            if(drawing&&isWord(reader,QLatin1StringView("drawing"))){
                if(!drawingAdded)paragraphHtml+=QStringLiteral("[Image omitted]");
                drawing=false;
            }
            if (isWord(reader, QLatin1StringView("rPr"))) runProperties = false;
            else if (isWord(reader, QLatin1StringView("r")) && run) { paragraphHtml += runHtml(currentRun); run = false; }
            else if (isWord(reader, QLatin1StringView("pPr"))) properties = false;
            else if (isWord(reader, QLatin1StringView("p")) && paragraph) {
                const QString tag = heading == 1 ? QStringLiteral("h1") : heading == 2 ? QStringLiteral("h2") : QStringLiteral("p");
                html += QLatin1Char('<') + tag;
                if(!paragraphAlignment.isEmpty())html+=QStringLiteral(" style=\"text-align:%1\"").arg(paragraphAlignment);
                html += QLatin1Char('>');
                if (numbered) html += QStringLiteral("&#8226; ");
                html += paragraphHtml.isEmpty() ? QStringLiteral("<br/>") : paragraphHtml;
                html += QStringLiteral("</") + tag + QLatin1Char('>');
                paragraph = false;
            } else if (isWord(reader, QLatin1StringView("tc"))) html += QStringLiteral("</td>");
            else if (isWord(reader, QLatin1StringView("tr"))) html += QStringLiteral("</tr>");
            else if (isWord(reader, QLatin1StringView("tbl"))) { html += QStringLiteral("</table>"); --tableDepth; }
            else if (isWord(reader, QLatin1StringView("body"))) body = false;
        }
    }
    html += QStringLiteral("</body></html>");
    okay = !reader.hasError() && sawParagraph && tableDepth == 0;
    return html;
}
void writeText(QXmlStreamWriter &writer, const QString &text) {
    QString segment;
    const auto flush = [&]() {
        if (segment.isEmpty()) return;
        writer.writeStartElement(QStringLiteral("w:t"));
        writer.writeAttribute(QStringLiteral("xml:space"), QStringLiteral("preserve"));
        writer.writeCharacters(segment);
        writer.writeEndElement();
        segment.clear();
    };
    for (QChar ch : text) {
        if (ch == QLatin1Char('\n')) { flush(); writer.writeEmptyElement(QStringLiteral("w:br")); }
        else if (ch == QLatin1Char('\t')) { flush(); writer.writeEmptyElement(QStringLiteral("w:tab")); }
        else segment += ch;
    }
    flush();
}
void writeRun(QXmlStreamWriter &writer, const QString &text, const QTextCharFormat &format) {
    if (text.isEmpty()) return;
    writer.writeStartElement(QStringLiteral("w:r"));
    writer.writeStartElement(QStringLiteral("w:rPr"));
    if (format.fontWeight() >= QFont::Bold) writer.writeEmptyElement(QStringLiteral("w:b"));
    if (format.fontItalic()) writer.writeEmptyElement(QStringLiteral("w:i"));
    if (format.fontUnderline()) { writer.writeEmptyElement(QStringLiteral("w:u")); writer.writeAttribute(QStringLiteral("w:val"), QStringLiteral("single")); }
    const QString family = format.fontFamilies().toStringList().value(0);
    if (!family.isEmpty()) { writer.writeEmptyElement(QStringLiteral("w:rFonts")); writer.writeAttribute(QStringLiteral("w:ascii"), family); writer.writeAttribute(QStringLiteral("w:hAnsi"), family); }
    const qreal size = format.fontPointSize();
    if (size > 0) { writer.writeEmptyElement(QStringLiteral("w:sz")); writer.writeAttribute(QStringLiteral("w:val"), QString::number(qRound(size * 2))); }
    const QColor color = format.foreground().color();
    if (color.isValid() && format.hasProperty(QTextFormat::ForegroundBrush)) {
        writer.writeEmptyElement(QStringLiteral("w:color"));
        writer.writeAttribute(QStringLiteral("w:val"), color.name().mid(1).toUpper());
    }
    writer.writeEndElement();
    writeText(writer, text);
    writer.writeEndElement();
}
void writeImageRun(QXmlStreamWriter &writer,const QTextImageFormat &image,const QString &relationship,int id){
    const qint64 width=qMax<qint64>(1,qRound64(image.width()*9525.0));
    const qint64 height=qMax<qint64>(1,qRound64(image.height()*9525.0));
    writer.writeStartElement("w:r");writer.writeStartElement("w:drawing");
    writer.writeStartElement("wp:inline");
    for(const QString &side:{"distT","distB","distL","distR"})writer.writeAttribute(side,"0");
    writer.writeEmptyElement("wp:extent");writer.writeAttribute("cx",QString::number(width));writer.writeAttribute("cy",QString::number(height));
    writer.writeEmptyElement("wp:docPr");writer.writeAttribute("id",QString::number(id));writer.writeAttribute("name","Picture "+QString::number(id));
    writer.writeStartElement("a:graphic");writer.writeStartElement("a:graphicData");
    writer.writeAttribute("uri","http://schemas.openxmlformats.org/drawingml/2006/picture");
    writer.writeStartElement("pic:pic");writer.writeStartElement("pic:nvPicPr");
    writer.writeEmptyElement("pic:cNvPr");writer.writeAttribute("id",QString::number(id));writer.writeAttribute("name","Picture "+QString::number(id));
    writer.writeEmptyElement("pic:cNvPicPr");writer.writeEndElement();
    writer.writeStartElement("pic:blipFill");writer.writeEmptyElement("a:blip");writer.writeAttribute("r:embed",relationship);
    writer.writeStartElement("a:stretch");writer.writeEmptyElement("a:fillRect");writer.writeEndElement();writer.writeEndElement();
    writer.writeStartElement("pic:spPr");writer.writeStartElement("a:xfrm");
    writer.writeEmptyElement("a:off");writer.writeAttribute("x","0");writer.writeAttribute("y","0");
    writer.writeEmptyElement("a:ext");writer.writeAttribute("cx",QString::number(width));writer.writeAttribute("cy",QString::number(height));
    writer.writeEndElement();writer.writeStartElement("a:prstGeom");writer.writeAttribute("prst","rect");
    writer.writeEmptyElement("a:avLst");writer.writeEndElement();writer.writeEndElement();
    writer.writeEndElement();writer.writeEndElement();writer.writeEndElement();writer.writeEndElement();writer.writeEndElement();writer.writeEndElement();
}
void writeParagraph(QXmlStreamWriter &writer, const QTextBlock &block,const QMap<QString,QString> &imageRelations) {
    writer.writeStartElement(QStringLiteral("w:p"));
    const int heading = block.blockFormat().headingLevel();
    const Qt::Alignment alignment=block.blockFormat().alignment();
    if (heading == 1 || heading == 2 || alignment.testFlag(Qt::AlignHCenter) || alignment.testFlag(Qt::AlignRight)) {
        writer.writeStartElement(QStringLiteral("w:pPr"));
        if(heading==1||heading==2){
            writer.writeEmptyElement(QStringLiteral("w:pStyle"));
            writer.writeAttribute(QStringLiteral("w:val"), heading == 1 ? QStringLiteral("Heading1") : QStringLiteral("Heading2"));
        }
        if(alignment.testFlag(Qt::AlignHCenter)||alignment.testFlag(Qt::AlignRight)){
            writer.writeEmptyElement("w:jc");writer.writeAttribute("w:val",alignment.testFlag(Qt::AlignHCenter)?"center":"right");
        }
        writer.writeEndElement();
    }
    if (QTextList *list = block.textList()) {
        const bool ordered = list->format().style() == QTextListFormat::ListDecimal;
        writeRun(writer, ordered ? QString::number(list->itemNumber(block) + 1) + QStringLiteral(". ") : QStringLiteral("• "), QTextCharFormat());
    }
    for (auto it = block.begin(); !it.atEnd(); ++it) {
        const QTextFragment fragment = it.fragment();
        if (!fragment.isValid()) continue;
        if (fragment.charFormat().isImageFormat()) {
            const QTextImageFormat image=fragment.charFormat().toImageFormat();
            const QString relation=imageRelations.value(image.name());
            if(relation.isEmpty())writeRun(writer,QStringLiteral("[Image omitted]"),QTextCharFormat());
            else writeImageRun(writer,image,relation,relation.mid(3).toInt());
        }
        else writeRun(writer, fragment.text(), fragment.charFormat());
    }
    writer.writeEndElement();
}
void writeTable(QXmlStreamWriter &writer, QTextTable *table,const QMap<QString,QString> &imageRelations) {
    writer.writeStartElement(QStringLiteral("w:tbl"));
    writer.writeStartElement(QStringLiteral("w:tblPr"));
    writer.writeEmptyElement(QStringLiteral("w:tblW"));
    writer.writeAttribute(QStringLiteral("w:w"), QStringLiteral("0"));
    writer.writeAttribute(QStringLiteral("w:type"), QStringLiteral("auto"));
    writer.writeEndElement();
    for (int row = 0; row < table->rows(); ++row) {
        writer.writeStartElement(QStringLiteral("w:tr"));
        for (int column = 0; column < table->columns(); ++column) {
            const QTextTableCell cell = table->cellAt(row, column);
            writer.writeStartElement(QStringLiteral("w:tc"));
            bool wroteParagraph = false;
            for (auto it = cell.begin(); !it.atEnd(); ++it) {
                const QTextBlock block = it.currentBlock();
                if (block.isValid()) { writeParagraph(writer, block,imageRelations); wroteParagraph = true; }
            }
            if (!wroteParagraph) writer.writeEmptyElement(QStringLiteral("w:p"));
            writer.writeEndElement();
        }
        writer.writeEndElement();
    }
    writer.writeEndElement();
}
QByteArray exportXml(const QTextDocument &document,const QMap<QString,QString> &imageRelations) {
    QByteArray xml;
    QXmlStreamWriter writer(&xml);
    writer.setAutoFormatting(false);
    writer.writeStartDocument();
    writer.writeStartElement(QStringLiteral("w:document"));
    writer.writeNamespace(QString::fromLatin1(wordNamespace), QStringLiteral("w"));
    writer.writeNamespace(QString::fromLatin1(relationNamespace),QStringLiteral("r"));
    writer.writeNamespace(QString::fromLatin1(drawingNamespace),QStringLiteral("a"));
    writer.writeNamespace(QString::fromLatin1(wordDrawingNamespace),QStringLiteral("wp"));
    writer.writeNamespace(QStringLiteral("http://schemas.openxmlformats.org/drawingml/2006/picture"),QStringLiteral("pic"));
    writer.writeStartElement(QStringLiteral("w:body"));
    for (auto it = document.rootFrame()->begin(); !it.atEnd(); ++it) {
        if (QTextTable *table = qobject_cast<QTextTable *>(it.currentFrame())) writeTable(writer, table,imageRelations);
        else if (const QTextBlock block = it.currentBlock(); block.isValid()) writeParagraph(writer, block,imageRelations);
    }
    writer.writeStartElement(QStringLiteral("w:sectPr"));
    writer.writeEmptyElement(QStringLiteral("w:pgSz"));
    writer.writeAttribute(QStringLiteral("w:w"), QStringLiteral("12240"));
    writer.writeAttribute(QStringLiteral("w:h"), QStringLiteral("15840"));
    writer.writeEmptyElement(QStringLiteral("w:pgMar"));
    writer.writeAttribute(QStringLiteral("w:top"), QStringLiteral("1080"));
    writer.writeAttribute(QStringLiteral("w:right"), QStringLiteral("1080"));
    writer.writeAttribute(QStringLiteral("w:bottom"), QStringLiteral("1080"));
    writer.writeAttribute(QStringLiteral("w:left"), QStringLiteral("1080"));
    writer.writeEndElement();
    writer.writeEndElement();
    writer.writeEndElement();
    writer.writeEndDocument();
    return xml;
}
}

bool WriteDocument::importDocx(const QUrl &url) {
    const QString path = url.isLocalFile() ? url.toLocalFile() : QString();
    if (path.isEmpty()) { setError(QStringLiteral("Choose a local DOCX file.")); return false; }
    int zipError = 0;
    zip_t *archive = zip_open(QFile::encodeName(path).constData(), ZIP_RDONLY, &zipError);
    if (!archive) { setError(QStringLiteral("Could not open the DOCX file.")); return false; }
    const QByteArray relationships = readEntry(archive, "_rels/.rels");
    QXmlStreamReader relationReader(relationships);
    QString mainPart;
    while (!relationReader.atEnd()) {
        relationReader.readNext();
        if (!relationReader.isStartElement() || relationReader.name() != QLatin1StringView("Relationship")) continue;
        const auto attributes = relationReader.attributes();
        if (attributes.value(QStringLiteral("Type")) == QLatin1StringView(officeRelationship))
            mainPart = attributes.value(QStringLiteral("Target")).toString();
    }
    if (mainPart.startsWith(QLatin1Char('/'))) mainPart.remove(0, 1);
    if (mainPart.isEmpty() || mainPart.contains(QStringLiteral("..")) || mainPart.contains(QLatin1Char('\\')) || mainPart.contains(QLatin1Char(':'))) {
        zip_close(archive);
        setError(QStringLiteral("This DOCX does not contain a supported main document."));
        return false;
    }
    const QByteArray xml = readEntry(archive, mainPart.toUtf8());
    const int slash=mainPart.lastIndexOf('/');
    const QString folder=slash>=0?mainPart.left(slash):QString();
    const QString relPart=(folder.isEmpty()?QString():folder+'/')+"_rels/"+mainPart.mid(slash+1)+".rels";
    QXmlStreamReader imageRels(readEntry(archive,relPart.toUtf8()));
    QHash<QString,QString> imageByRelation;
    QMap<QString,QByteArray> importedImages;
    qsizetype totalImageBytes=0;
    while(!imageRels.atEnd()){
        imageRels.readNext();
        if(!imageRels.isStartElement()||imageRels.name()!=QLatin1StringView("Relationship"))continue;
        const auto attributes=imageRels.attributes();
        if(!attributes.value("Type").toString().endsWith("/image")||attributes.value("TargetMode")=="External"||importedImages.size()>=50)continue;
        const QString part=OfficeZip::resolve(folder,attributes.value("Target").toString());
        if(part.isEmpty())continue;
        QByteArray bytes=OfficeZip::read(archive,part,8*1024*1024);
        if(bytes.isEmpty())continue;
        QBuffer source(&bytes);source.open(QIODevice::ReadOnly);QImageReader imageReader(&source);
        QSize original=imageReader.size();
        if(!original.isValid()||original.width()>10000||original.height()>10000)continue;
        if(original.width()>2400||original.height()>2400)imageReader.setScaledSize(original.scaled(2400,2400,Qt::KeepAspectRatio));
        QImage image=imageReader.read();if(image.isNull())continue;
        QByteArray png;QBuffer output(&png);output.open(QIODevice::WriteOnly);
        if(!image.save(&output,"PNG")||png.size()>8*1024*1024||totalImageBytes+png.size()>40*1024*1024)continue;
        const QString name="media/"+QUuid::createUuid().toString(QUuid::WithoutBraces)+".png";
        importedImages.insert(name,png);totalImageBytes+=png.size();
        imageByRelation.insert(attributes.value("Id").toString(),name);
    }
    zip_close(archive);
    bool okay = false;
    const QString html = readDocumentHtml(xml,imageByRelation,importedImages,okay);
    if (!okay) { setError(QStringLiteral("Could not read the DOCX text and formatting.")); return false; }
    m_document.clear();
    m_images=importedImages;
    for(auto it=m_images.cbegin();it!=m_images.cend();++it)
        m_document.addResource(QTextDocument::ImageResource,QUrl(it.key()),QImage::fromData(it.value(),"PNG"));
    m_document.setHtml(html);
    m_topMargin=m_bottomMargin=m_leftMargin=m_rightMargin=54;
    m_headerText.clear();m_footerText.clear();m_showPageNumbers=false;
    applyPageLayout();
    reflowAlignedImages();
    m_pageDirty=false;
    m_document.setModified(true);
    m_path.clear();
    m_hasDocument = true;
    removeSessionRecovery();
    setError({});
    emit stateChanged();
    return true;
}

bool WriteDocument::exportDocx(const QUrl &url) {
    QString path = url.isLocalFile() ? url.toLocalFile() : QString();
    if (path.isEmpty()) { setError(QStringLiteral("Choose a local DOCX destination.")); return false; }
    if (!path.endsWith(QStringLiteral(".docx"), Qt::CaseInsensitive)) path += QStringLiteral(".docx");
    QTemporaryFile temporary(QDir::tempPath() + QStringLiteral("/nexus-docx-XXXXXX"));
    if (!temporary.open()) { setError(QStringLiteral("Could not prepare a DOCX file.")); return false; }
    const QString tempPath = temporary.fileName();
    temporary.close();
    int zipError = 0;
    zip_t *archive = zip_open(QFile::encodeName(tempPath).constData(), ZIP_CREATE | ZIP_TRUNCATE, &zipError);
    if (!archive) { setError(QStringLiteral("Could not create the DOCX package.")); return false; }
    const QByteArray contentTypes = R"(<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>)";
    const QByteArray relationships = R"(<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>)";
    QMap<QString,QString> imageRelations;
    QMap<QString,QByteArray> imageParts;
    int imageNumber=0;
    for(auto it=m_images.cbegin();it!=m_images.cend();++it){
        const QString number=QString::number(++imageNumber);
        imageRelations.insert(it.key(),"rId"+QString::number(imageNumber+1));
        imageParts.insert("word/media/image"+number+".png",it.value());
    }
    QByteArray documentRelationships;
    {
        QXmlStreamWriter writer(&documentRelationships);writer.writeStartDocument();
        writer.writeStartElement("Relationships");writer.writeDefaultNamespace("http://schemas.openxmlformats.org/package/2006/relationships");
        writer.writeEmptyElement("Relationship");writer.writeAttribute("Id","rId1");
        writer.writeAttribute("Type",QString::fromLatin1(relationNamespace)+"/styles");writer.writeAttribute("Target","styles.xml");
        for(int index=1;index<=imageNumber;++index){
            writer.writeEmptyElement("Relationship");writer.writeAttribute("Id","rId"+QString::number(index+1));
            writer.writeAttribute("Type",QString::fromLatin1(relationNamespace)+"/image");
            writer.writeAttribute("Target","media/image"+QString::number(index)+".png");
        }
        writer.writeEndElement();writer.writeEndDocument();
    }
    const QByteArray styles = R"(<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:rPr><w:rFonts w:ascii="Noto Sans" w:hAnsi="Noto Sans"/><w:sz w:val="22"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:before="240" w:after="120"/></w:pPr><w:rPr><w:b/><w:sz w:val="40"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:before="180" w:after="90"/></w:pPr><w:rPr><w:b/><w:sz w:val="32"/></w:rPr></w:style></w:styles>)";
    const QByteArray documentXml = exportXml(m_document,imageRelations);
    bool packaged = addEntry(archive, "[Content_Types].xml", contentTypes) &&
                          addEntry(archive, "_rels/.rels", relationships) &&
                          addEntry(archive, "word/document.xml", documentXml) &&
                          addEntry(archive, "word/_rels/document.xml.rels", documentRelationships) &&
                          addEntry(archive, "word/styles.xml", styles);
    for(auto it=imageParts.cbegin();packaged&&it!=imageParts.cend();++it){
        const QByteArray name=it.key().toUtf8();packaged=addEntry(archive,name.constData(),it.value());
    }
    if (!packaged) { zip_discard(archive); setError(QStringLiteral("Could not package the DOCX file.")); return false; }
    if (zip_close(archive) != 0) { zip_discard(archive); setError(QStringLiteral("Could not finish the DOCX package.")); return false; }
    QFile source(tempPath);
    if (!source.open(QIODevice::ReadOnly)) { setError(QStringLiteral("Could not read the temporary DOCX file.")); return false; }
    const QByteArray data = source.readAll();
    QSaveFile destination(path);
    if (!destination.open(QIODevice::WriteOnly) || destination.write(data) != data.size() || !destination.commit()) {
        setError(QStringLiteral("Could not export the DOCX file.")); return false;
    }
    setError({});
    return true;
}
