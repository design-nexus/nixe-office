#include "WriteDocument.h"

#include <QFile>
#include <QBuffer>
#include <QProcess>
#include <QStandardPaths>
#include <QGuiApplication>
#include <QTemporaryDir>
#include <QTextCursor>
#include <QTextBlock>
#include <QTextTable>
#include <QImage>
#include <QRegularExpression>
#include <QtTest>
#include <zip.h>
#include <QXmlStreamReader>

class WriteTests : public QObject {
    Q_OBJECT
private slots:
    void nativeRoundTrip();
    void rejectsInvalidFileWithoutLosingDocument();
    void formatsSelectedText();
    void fontFormattingRoundTrip();
    void templatesCreateEditableDocuments();
    void docxImportExportRoundTrip();
    void rejectsBadDocxWithoutLosingText();
    void importsStandardWordXmlFixture();
    void recoversUnsavedText();
    void paginatesLongDocument();
    void imageTableAndPdfRoundTrip();
    void imagePlacementAndDocxRoundTrip();
    void importsExternalDocxImage();
    void pageSetupNativeRecoveryAndPdf();
    void opensLegacyPageDefaults();
};

void WriteTests::nativeRoundTrip() {
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    WriteDocument writer;
    writer.document()->setHtml(QStringLiteral("<h1>House notes</h1><p><b>Paint</b> the kitchen.</p>"));
    QVERIFY(writer.dirty());
    const QString path = dir.path() + QStringLiteral("/notes.nwrite");
    QVERIFY2(writer.saveAs(QUrl::fromLocalFile(path)), qPrintable(writer.errorString()));
    QVERIFY(!writer.dirty());
    QCOMPARE(writer.path(), path);

    WriteDocument reader;
    QVERIFY2(reader.open(QUrl::fromLocalFile(path)), qPrintable(reader.errorString()));
    QVERIFY(reader.document()->toPlainText().contains(QStringLiteral("Paint the kitchen")));
    QVERIFY(reader.document()->toHtml().contains(QStringLiteral("font-weight:700")));
    QVERIFY(!reader.dirty());
}

void WriteTests::rejectsInvalidFileWithoutLosingDocument() {
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString path = dir.path() + QStringLiteral("/bad.nwrite");
    QFile file(path);
    QVERIFY(file.open(QIODevice::WriteOnly));
    file.write("not a zip file");
    file.close();
    WriteDocument writer;
    writer.document()->setPlainText(QStringLiteral("Keep this text"));
    QVERIFY(!writer.open(QUrl::fromLocalFile(path)));
    QCOMPARE(writer.document()->toPlainText(), QStringLiteral("Keep this text"));
    QVERIFY(!writer.errorString().isEmpty());
}

void WriteTests::formatsSelectedText() {
    WriteDocument writer;
    writer.document()->setPlainText(QStringLiteral("Hello world"));
    writer.toggleBold(0, 5);
    QTextCursor cursor(writer.document());
    cursor.setPosition(2);
    QCOMPARE(cursor.charFormat().fontWeight(), static_cast<int>(QFont::Bold));
    writer.setHeading(1, 1);
    QCOMPARE(writer.document()->firstBlock().blockFormat().headingLevel(), 1);
}

void WriteTests::fontFormattingRoundTrip() {
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    WriteDocument writer;
    writer.document()->setPlainText(QStringLiteral("Colorful letter"));
    writer.setFontFamily(0, 8, QStringLiteral("Noto Serif"));
    writer.setFontSize(0, 8, 18);
    writer.setFontColor(0, 8, QColor(QStringLiteral("#cc3366")));
    const QString path = dir.path() + QStringLiteral("/font.nwrite");
    QVERIFY(writer.saveAs(QUrl::fromLocalFile(path)));
    WriteDocument reopened;
    QVERIFY(reopened.open(QUrl::fromLocalFile(path)));
    QTextCursor cursor(reopened.document());
    cursor.setPosition(2);
    QCOMPARE(cursor.charFormat().fontFamilies().toStringList().first(), QStringLiteral("Noto Serif"));
    QCOMPARE(cursor.charFormat().fontPointSize(), 18.0);
    QCOMPARE(cursor.charFormat().foreground().color(), QColor(QStringLiteral("#cc3366")));
}

void WriteTests::templatesCreateEditableDocuments() {
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    WriteDocument writer;
    for (const QString &id : {QStringLiteral("letter"), QStringLiteral("recipe"), QStringLiteral("journal")}) {
        QVERIFY(writer.createTemplate(id));
        QVERIFY(writer.path().isEmpty());
        QVERIFY(writer.dirty());
        QVERIFY(writer.document()->toPlainText().size() > 50);
    }
    const QString before = writer.document()->toPlainText();
    QVERIFY(!writer.createTemplate(QStringLiteral("unknown")));
    QCOMPARE(writer.document()->toPlainText(), before);
    const QString path = dir.path() + QStringLiteral("/journal.nwrite");
    QVERIFY(writer.saveAs(QUrl::fromLocalFile(path)));
    WriteDocument reopened;
    QVERIFY(reopened.open(QUrl::fromLocalFile(path)));
    QVERIFY(reopened.document()->toPlainText().contains(QStringLiteral("Highlights")));
}

void WriteTests::docxImportExportRoundTrip() {
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    WriteDocument writer;
    writer.newDocument();
    writer.document()->setHtml(QStringLiteral("<h1>Holiday notes</h1><p><b><span style='color:#cc3366'>Hello</span></b> family</p><table border='1'><tr><td>Day</td><td>Plan</td></tr><tr><td>1</td><td>Beach</td></tr></table>"));
    const QString docxPath = dir.path() + QStringLiteral("/holiday.docx");
    QVERIFY2(writer.exportDocx(QUrl::fromLocalFile(docxPath)), qPrintable(writer.errorString()));
    QVERIFY(writer.path().isEmpty());
    QVERIFY(writer.dirty());
    int zipError = 0;
    zip_t *archive = zip_open(QFile::encodeName(docxPath).constData(), ZIP_RDONLY, &zipError);
    QVERIFY(archive != nullptr);
    QVERIFY(zip_name_locate(archive, "[Content_Types].xml", 0) >= 0);
    QVERIFY(zip_name_locate(archive, "_rels/.rels", 0) >= 0);
    QVERIFY(zip_name_locate(archive, "word/document.xml", 0) >= 0);
    QVERIFY(zip_name_locate(archive, "word/styles.xml", 0) >= 0);
    QVERIFY(zip_name_locate(archive, "word/_rels/document.xml.rels", 0) >= 0);
    zip_file_t *part = zip_fopen(archive, "word/document.xml", 0);
    QVERIFY(part != nullptr);
    QByteArray xml(65536, '\0');
    const zip_int64_t count = zip_fread(part, xml.data(), xml.size());
    QVERIFY(count > 0);
    xml.resize(count);
    zip_fclose(part);
    zip_close(archive);
    QXmlStreamReader reader(xml);
    while (!reader.atEnd()) reader.readNext();
    QVERIFY(!reader.hasError());
    WriteDocument imported;
    QVERIFY2(imported.importDocx(QUrl::fromLocalFile(docxPath)), qPrintable(imported.errorString()));
    QVERIFY(imported.path().isEmpty());
    QVERIFY(imported.dirty());
    QVERIFY(imported.document()->toPlainText().contains(QStringLiteral("Holiday notes")));
    QVERIFY(imported.document()->toPlainText().contains(QStringLiteral("Beach")));
    QVERIFY(imported.document()->toHtml().contains(QStringLiteral("<table")));
    QTextCursor found = imported.document()->find(QStringLiteral("Hello"));
    QVERIFY(!found.isNull());
    QCOMPARE(found.charFormat().fontWeight(), static_cast<int>(QFont::Bold));
    QCOMPARE(found.charFormat().foreground().color(), QColor(QStringLiteral("#cc3366")));
}

void WriteTests::rejectsBadDocxWithoutLosingText() {
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString path = dir.path() + QStringLiteral("/bad.docx");
    QFile file(path);
    QVERIFY(file.open(QIODevice::WriteOnly));
    file.write("not a ZIP package");
    file.close();
    WriteDocument writer;
    writer.document()->setPlainText(QStringLiteral("Keep my letter"));
    QVERIFY(!writer.importDocx(QUrl::fromLocalFile(path)));
    QCOMPARE(writer.document()->toPlainText(), QStringLiteral("Keep my letter"));
}

void WriteTests::importsStandardWordXmlFixture() {
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString path = dir.path() + QStringLiteral("/outside.docx");
    int error = 0;
    zip_t *archive = zip_open(QFile::encodeName(path).constData(), ZIP_CREATE | ZIP_TRUNCATE, &error);
    QVERIFY(archive != nullptr);
    const QByteArray rels = R"(<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId42" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="/word/document.xml"/></Relationships>)";
    const QByteArray document = R"(<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:pPr><w:pStyle w:val="Heading2"/></w:pPr><w:r><w:rPr><w:b/><w:i/><w:color w:val="123456"/><w:sz w:val="28"/></w:rPr><w:t>Outside</w:t></w:r><w:r><w:t xml:space="preserve"> document</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>Cell A</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Cell B</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>)";
    const auto add = [archive](const char *name, const QByteArray &data) {
        zip_source_t *source = zip_source_buffer(archive, data.constData(), data.size(), 0);
        if (!source) return false;
        if (zip_file_add(archive, name, source, 0) < 0) { zip_source_free(source); return false; }
        return true;
    };
    QVERIFY(add("_rels/.rels", rels));
    QVERIFY(add("word/document.xml", document));
    QCOMPARE(zip_close(archive), 0);
    WriteDocument imported;
    QVERIFY2(imported.importDocx(QUrl::fromLocalFile(path)), qPrintable(imported.errorString()));
    QVERIFY(imported.document()->toPlainText().contains(QStringLiteral("Outside document")));
    QVERIFY(imported.document()->toPlainText().contains(QStringLiteral("Cell B")));
    QCOMPARE(imported.document()->firstBlock().blockFormat().headingLevel(), 2);
    QTextCursor found = imported.document()->find(QStringLiteral("Outside"));
    QCOMPARE(found.charFormat().fontWeight(), static_cast<int>(QFont::Bold));
    QVERIFY(found.charFormat().fontItalic());
    QCOMPARE(found.charFormat().fontPointSize(), 14.0);
    QCOMPARE(found.charFormat().foreground().color(), QColor(QStringLiteral("#123456")));
}

void WriteTests::recoversUnsavedText() {
    WriteDocument writer;
    writer.newDocument();
    writer.document()->setPlainText(QStringLiteral("Unfinished letter"));
    QVERIFY(writer.dirty());
    writer.writeRecoveryNow();

    WriteDocument reopened;
    QVERIFY(reopened.recoveryAvailable());
    QVERIFY(reopened.restoreRecovery());
    QCOMPARE(reopened.document()->toPlainText(), QStringLiteral("Unfinished letter"));
    QVERIFY(reopened.dirty());
    reopened.discardRecovery();
    writer.discardSessionRecovery();
}

void WriteTests::paginatesLongDocument() {
    WriteDocument writer;
    QString paragraphs;
    for (int i = 0; i < 100; ++i)
        paragraphs += QStringLiteral("Paragraph %1: a few words for the page.\n").arg(i);
    writer.document()->setPlainText(paragraphs);
    QVERIFY(writer.pageCount() > 1);
}

void WriteTests::imageTableAndPdfRoundTrip() {
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString imagePath = dir.path() + QStringLiteral("/photo.png");
    QImage image(48, 32, QImage::Format_RGB32);
    image.fill(Qt::red);
    QVERIFY(image.save(imagePath));

    WriteDocument writer;
    writer.newDocument();
    writer.document()->setPlainText(QStringLiteral("A picture and a table"));
    QVERIFY2(writer.insertImage(0, QUrl::fromLocalFile(imagePath)), qPrintable(writer.errorString()));
    writer.insertTable(writer.document()->characterCount() - 1, 3, 3);
    QVERIFY(writer.document()->toHtml().contains(QStringLiteral("media/")));
    QVERIFY(writer.document()->toHtml().contains(QStringLiteral("<table")));

    const QString nativePath = dir.path() + QStringLiteral("/sample.nwrite");
    QVERIFY2(writer.saveAs(QUrl::fromLocalFile(nativePath)), qPrintable(writer.errorString()));
    WriteDocument reader;
    QVERIFY2(reader.open(QUrl::fromLocalFile(nativePath)), qPrintable(reader.errorString()));
    QVERIFY(reader.document()->toHtml().contains(QStringLiteral("media/")));
    QVERIFY(reader.document()->toHtml().contains(QStringLiteral("<table")));
    const QRegularExpression imageName(QStringLiteral("src=\"(media/[^\"]+\\.png)\""));
    const auto match = imageName.match(reader.document()->toHtml());
    QVERIFY(match.hasMatch());
    QVERIFY(!reader.document()->resource(QTextDocument::ImageResource, QUrl(match.captured(1))).value<QImage>().isNull());

    const QString pdfPath = dir.path() + QStringLiteral("/sample.pdf");
    QVERIFY2(reader.exportPdf(QUrl::fromLocalFile(pdfPath)), qPrintable(reader.errorString()));
    QFile pdf(pdfPath);
    QVERIFY(pdf.open(QIODevice::ReadOnly));
    QCOMPARE(pdf.read(5), QByteArray("%PDF-"));

    writer.document()->setModified(true);
    writer.writeRecoveryNow();
    WriteDocument recovered;
    QVERIFY(recovered.recoveryAvailable());
    QVERIFY(recovered.restoreRecovery());
    QVERIFY(!recovered.document()->resource(QTextDocument::ImageResource, QUrl(match.captured(1))).value<QImage>().isNull());
    recovered.discardRecovery();
    writer.discardSessionRecovery();
}

void WriteTests::imagePlacementAndDocxRoundTrip(){
    QTemporaryDir dir;QVERIFY(dir.isValid());
    QImage photo(240,120,QImage::Format_RGB32);photo.fill(QColor("#4488cc"));
    const QString source=dir.path()+"/photo.png";QVERIFY(photo.save(source));
    WriteDocument writer;writer.newDocument();writer.document()->setPlainText("Family photo");
    QVERIFY(writer.insertImage(0,QUrl::fromLocalFile(source)));
    QVERIFY(writer.imageAt(0).value("found").toBool());
    QVERIFY(writer.setImageWidth(0,180));
    QVERIFY(writer.setImageAlignment(0,"center"));
    QCOMPARE(writer.imageAt(0).value("width").toInt(),180);
    QCOMPARE(writer.imageAt(0).value("height").toInt(),90);
    QCOMPARE(writer.imageAt(0).value("alignment").toString(),QString("center"));
    QCOMPARE(writer.document()->firstBlock().blockFormat().leftMargin(),216.0);
    const QString path=dir.path()+"/picture.docx";
    QVERIFY2(writer.exportDocx(QUrl::fromLocalFile(path)),qPrintable(writer.errorString()));
    int error=0;zip_t *archive=zip_open(QFile::encodeName(path).constData(),ZIP_RDONLY,&error);QVERIFY(archive);
    QVERIFY(zip_name_locate(archive,"word/media/image1.png",0)>=0);
    zip_file_t *part=zip_fopen(archive,"word/document.xml",0);QVERIFY(part);
    QByteArray xml(65536,'\0');const zip_int64_t count=zip_fread(part,xml.data(),xml.size());QVERIFY(count>0);xml.resize(count);
    zip_fclose(part);zip_close(archive);
    QVERIFY(xml.contains("r:embed=\"rId2\""));QVERIFY(xml.contains("w:jc w:val=\"center\""));
    QXmlStreamReader parsed(xml);while(!parsed.atEnd())parsed.readNext();QVERIFY(!parsed.hasError());
    WriteDocument imported;QVERIFY2(imported.importDocx(QUrl::fromLocalFile(path)),qPrintable(imported.errorString()));
    const QVariantMap importedImage=imported.imageAt(0);
    QVERIFY(importedImage.value("found").toBool());
    QCOMPARE(importedImage.value("width").toInt(),180);
    QCOMPARE(importedImage.value("height").toInt(),90);
    QCOMPARE(importedImage.value("alignment").toString(),QString("center"));
    const QString native=dir.path()+"/picture.nwrite";QVERIFY(imported.saveAs(QUrl::fromLocalFile(native)));
    WriteDocument reopened;QVERIFY(reopened.open(QUrl::fromLocalFile(native)));
    QCOMPARE(reopened.imageAt(0).value("width").toInt(),180);
    QCOMPARE(reopened.imageAt(0).value("alignment").toString(),QString("center"));
    reopened.setPageLayout(54,54,72,72,"","",false);
    QCOMPARE(reopened.document()->firstBlock().blockFormat().leftMargin(),198.0);
    reopened.setImageWidth(0,160);reopened.writeRecoveryNow();
    WriteDocument recovered;QVERIFY(recovered.recoveryAvailable());QVERIFY(recovered.restoreRecovery());
    QCOMPARE(recovered.imageAt(0).value("width").toInt(),160);
    QCOMPARE(recovered.imageAt(0).value("alignment").toString(),QString("center"));
    QCOMPARE(recovered.document()->firstBlock().blockFormat().leftMargin(),208.0);
    recovered.discardRecovery();writer.discardSessionRecovery();imported.discardSessionRecovery();reopened.discardSessionRecovery();
}

void WriteTests::importsExternalDocxImage(){
    QTemporaryDir dir;QVERIFY(dir.isValid());
    const QString path=dir.path()+"/outside-image.docx";
    QImage photo(100,50,QImage::Format_RGB32);photo.fill(Qt::green);
    QByteArray jpeg;QBuffer buffer(&jpeg);buffer.open(QIODevice::WriteOnly);QVERIFY(photo.save(&buffer,"JPG"));
    int error=0;zip_t *archive=zip_open(QFile::encodeName(path).constData(),ZIP_CREATE|ZIP_TRUNCATE,&error);QVERIFY(archive);
    const auto add=[archive](const char *name,const QByteArray &data){
        zip_source_t *source=zip_source_buffer(archive,data.constData(),data.size(),0);
        if(!source)return false;if(zip_file_add(archive,name,source,0)<0){zip_source_free(source);return false;}return true;
    };
    const QByteArray root=R"(<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>)";
    const QByteArray rels=R"(<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/photo.jpg"/></Relationships>)";
    const QByteArray xml=R"(<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><w:body><w:p><w:pPr><w:jc w:val="right"/></w:pPr><w:r><w:drawing><wp:inline><wp:extent cx="1905000" cy="952500"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:blipFill><a:blip r:embed="rId7"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p></w:body></w:document>)";
    QVERIFY(add("_rels/.rels",root));QVERIFY(add("word/_rels/document.xml.rels",rels));
    QVERIFY(add("word/document.xml",xml));QVERIFY(add("word/media/photo.jpg",jpeg));QCOMPARE(zip_close(archive),0);
    WriteDocument imported;QVERIFY2(imported.importDocx(QUrl::fromLocalFile(path)),qPrintable(imported.errorString()));
    const QVariantMap info=imported.imageAt(0);QVERIFY(info.value("found").toBool());
    QCOMPARE(info.value("width").toInt(),200);QCOMPARE(info.value("height").toInt(),100);
    QCOMPARE(info.value("alignment").toString(),QString("right"));
    imported.discardSessionRecovery();
}

void WriteTests::pageSetupNativeRecoveryAndPdf() {
    QTemporaryDir dir; QVERIFY(dir.isValid());
    WriteDocument writer;writer.newDocument();
    QString text;for(int i=0;i<90;++i)text+=QString("Entry %1: family trip notes and directions.\n").arg(i);
    writer.document()->setPlainText(text);
    writer.setPageLayout(72,84,90,66,"Family Journal","Private copy",true);
    QVERIFY(writer.dirty());QCOMPARE(writer.leftMargin(),90);QVERIFY(writer.pageCount()>1);
    const QString native=dir.path()+"/journal.nwrite";
    QVERIFY2(writer.saveAs(QUrl::fromLocalFile(native)),qPrintable(writer.errorString()));
    WriteDocument opened;QVERIFY2(opened.open(QUrl::fromLocalFile(native)),qPrintable(opened.errorString()));
    QCOMPARE(opened.topMargin(),72);QCOMPARE(opened.bottomMargin(),84);
    QCOMPARE(opened.leftMargin(),90);QCOMPARE(opened.rightMargin(),66);
    QCOMPARE(opened.headerText(),QString("Family Journal"));
    QCOMPARE(opened.footerText(),QString("Private copy"));QVERIFY(opened.showPageNumbers());
    const QString pdf=dir.path()+"/journal.pdf";
    QVERIFY2(opened.exportPdf(QUrl::fromLocalFile(pdf)),qPrintable(opened.errorString()));
    QFile file(pdf);QVERIFY(file.open(QIODevice::ReadOnly));QCOMPARE(file.read(5),QByteArray("%PDF-"));
    const QString pdftotext=QStandardPaths::findExecutable("pdftotext");
    if(!pdftotext.isEmpty()){
        QProcess process;process.start(pdftotext,{"-layout",pdf,"-"});QVERIFY(process.waitForFinished());
        QCOMPARE(process.exitCode(),0);QString extracted=QString::fromUtf8(process.readAllStandardOutput());
        QVERIFY(extracted.contains("Family Journal"));QVERIFY(extracted.contains("Private copy"));
        QVERIFY(extracted.contains("Entry 0"));QVERIFY(extracted.contains("Entry 89"));
        QCOMPARE(extracted.count("Family Journal"),opened.pageCount());
    }
    const QString pdftohtml=QStandardPaths::findExecutable("pdftohtml");
    if(!pdftohtml.isEmpty()){
        QProcess process;process.start(pdftohtml,{"-xml","-hidden","-i",pdf,dir.path()+"/layout"});
        QVERIFY(process.waitForFinished());QCOMPARE(process.exitCode(),0);
        QFile layout(dir.path()+"/layout.xml");QVERIFY(layout.open(QIODevice::ReadOnly));
        QVERIFY(layout.readAll().contains("color=\"#202329\""));
    }
    opened.setPageLayout(96,96,96,96,"Recovered header","",false);
    opened.writeRecoveryNow();WriteDocument recovered;QVERIFY(recovered.recoveryAvailable());
    QVERIFY(recovered.restoreRecovery());QCOMPARE(recovered.headerText(),QString("Recovered header"));
    QCOMPARE(recovered.topMargin(),96);QVERIFY(!recovered.showPageNumbers());
    recovered.discardRecovery();opened.discardSessionRecovery();writer.discardSessionRecovery();
}

void WriteTests::opensLegacyPageDefaults() {
    QTemporaryDir dir;QVERIFY(dir.isValid());QString path=dir.path()+"/old.nwrite";
    int error=0;zip_t *archive=zip_open(QFile::encodeName(path).constData(),ZIP_CREATE,&error);QVERIFY(archive);
    auto add=[&](const char *name,const QByteArray &data){zip_source_t *source=zip_source_buffer(archive,data.constData(),data.size(),0);QVERIFY(source);QVERIFY(zip_file_add(archive,name,source,0)>=0);};
    const QByteArray manifest=R"({"format":"nexus-write","version":1,"content":"document.html"})";
    const QByteArray html="<html><body><p>Old letter</p></body></html>";
    add("manifest.json",manifest);add("document.html",html);QCOMPARE(zip_close(archive),0);
    WriteDocument opened;QVERIFY2(opened.open(QUrl::fromLocalFile(path)),qPrintable(opened.errorString()));
    QCOMPARE(opened.topMargin(),54);QCOMPARE(opened.leftMargin(),54);
    QVERIFY(opened.headerText().isEmpty());QVERIFY(!opened.showPageNumbers());
    QVERIFY(opened.document()->toPlainText().contains("Old letter"));
}

int main(int argc, char **argv) {
    QTemporaryDir dataDir;
    if (!dataDir.isValid()) return 1;
    qputenv("XDG_DATA_HOME", dataDir.path().toUtf8());
    qputenv("QT_QPA_PLATFORM", "offscreen");
    QGuiApplication app(argc, argv);
    WriteTests tests;
    return QTest::qExec(&tests, argc, argv);
}
#include "WriteTests.moc"
