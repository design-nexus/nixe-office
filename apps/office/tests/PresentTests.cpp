#include "PresentDocument.h"

#include <QFile>
#include <QImage>
#include <QProcess>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QtTest>

class PresentTests : public QObject {
    Q_OBJECT
private slots:
    void editsSlidesAndRoundTrips();
    void typographyAndColorsRoundTrip();
    void undoRedoAndRecovery();
    void templatesCreateWorkingDecks();
    void rejectsInvalidFileWithoutLosingSlides();
    void exportsSlidesAsPdf();
    void pptxRoundTrip();
    void slideObjectsRoundTrip();
    void shapesAndAlignmentRoundTrip();
};

void PresentTests::editsSlidesAndRoundTrips() {
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    PresentDocument deck;
    deck.newDocument();
    QCOMPARE(deck.slideCount(), 1);
    QCOMPARE(deck.selectedIndex(), 0);
    QVERIFY(!deck.dirty());
    deck.setTitle(QStringLiteral("Holiday plans"));
    deck.setBody(QStringLiteral("Summer 2027"));
    deck.setNotes(QStringLiteral("Introduce the itinerary."));
    deck.setStyle(2);
    deck.addSlide();
    QCOMPARE(deck.slideCount(), 2);
    QCOMPARE(deck.currentLayout(), 1);
    QCOMPARE(deck.currentStyle(), 2);
    deck.setTitle(QStringLiteral("The route"));
    deck.setBody(QStringLiteral("Fly to the coast\nDrive north"));
    deck.duplicateSlide();
    QCOMPARE(deck.slideCount(), 3);
    QCOMPARE(deck.currentTitle(), QStringLiteral("The route"));
    deck.moveSlide(-1);
    QCOMPARE(deck.selectedIndex(), 1);
    deck.deleteSlide();
    QCOMPARE(deck.slideCount(), 2);
    QVERIFY(deck.dirty());

    const QString path = dir.path() + QStringLiteral("/holiday.npresent");
    QVERIFY2(deck.saveAs(QUrl::fromLocalFile(path)), qPrintable(deck.errorString()));
    QVERIFY(!deck.dirty());
    PresentDocument reopened;
    QVERIFY2(reopened.open(QUrl::fromLocalFile(path)), qPrintable(reopened.errorString()));
    QCOMPARE(reopened.slideCount(), 2);
    QCOMPARE(reopened.currentTitle(), QStringLiteral("Holiday plans"));
    QCOMPARE(reopened.currentBody(), QStringLiteral("Summer 2027"));
    QCOMPARE(reopened.currentNotes(), QStringLiteral("Introduce the itinerary."));
    QCOMPARE(reopened.currentStyle(), 2);
    QCOMPARE(reopened.slideAt(0).value(QStringLiteral("title")).toString(), QStringLiteral("Holiday plans"));
    QVERIFY(reopened.slideAt(99).isEmpty());
    reopened.selectSlide(1);
    QCOMPARE(reopened.currentTitle(), QStringLiteral("The route"));
    QCOMPARE(reopened.currentLayout(), 1);
    QVERIFY(!reopened.dirty());
    reopened.deleteSlide();
    reopened.deleteSlide();
    QCOMPARE(reopened.slideCount(), 1);
    QCOMPARE(reopened.currentTitle(), QString());
    QCOMPARE(reopened.currentBody(), QString());
}

void PresentTests::typographyAndColorsRoundTrip() {
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    PresentDocument deck;
    deck.newDocument();
    deck.setFontFamily(QStringLiteral("Noto Serif"));
    deck.setFontColor(QStringLiteral("#f0e0c0"));
    deck.setBackgroundColor(QStringLiteral("#182034"));
    deck.setBold(true);
    deck.setItalic(true);
    deck.setTitleSize(62);
    deck.setBodySize(30);
    const QString path = dir.path() + QStringLiteral("/style.npresent");
    QVERIFY(deck.saveAs(QUrl::fromLocalFile(path)));
    PresentDocument reopened;
    QVERIFY(reopened.open(QUrl::fromLocalFile(path)));
    QCOMPARE(reopened.currentFontFamily(), QStringLiteral("Noto Serif"));
    QCOMPARE(reopened.currentFontColor(), QStringLiteral("#f0e0c0"));
    QCOMPARE(reopened.currentBackgroundColor(), QStringLiteral("#182034"));
    QVERIFY(reopened.currentBold());
    QVERIFY(reopened.currentItalic());
    QCOMPARE(reopened.currentTitleSize(), 62);
    QCOMPARE(reopened.currentBodySize(), 30);
    QVERIFY(!reopened.dirty());
}

void PresentTests::undoRedoAndRecovery() {
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    PresentDocument deck;
    deck.newDocument();
    deck.setTitle(QStringLiteral("Trip"));
    const QString path = dir.path() + QStringLiteral("/history.npresent");
    QVERIFY(deck.saveAs(QUrl::fromLocalFile(path)));
    deck.setFontColor(QStringLiteral("#ffeecc"));
    deck.addSlide();
    deck.setTitle(QStringLiteral("R"));
    deck.setTitle(QStringLiteral("Route"));
    QVERIFY(deck.canUndo());
    deck.undo();
    QCOMPARE(deck.currentTitle(), QString());
    deck.undo();
    QCOMPARE(deck.slideCount(), 1);
    deck.undo();
    QCOMPARE(deck.currentFontColor(), QString());
    QVERIFY(!deck.dirty());
    deck.redo();
    QCOMPARE(deck.currentFontColor(), QStringLiteral("#ffeecc"));
    deck.addSlide();
    deck.setTitle(QStringLiteral("Route"));
    QVERIFY(!deck.canRedo());
    deck.writeRecoveryNow();
    PresentDocument recovered;
    QVERIFY(recovered.recoveryAvailable());
    QVERIFY(recovered.restoreRecovery());
    QCOMPARE(recovered.path(), path);
    QCOMPARE(recovered.slideCount(), 2);
    QCOMPARE(recovered.currentTitle(), QStringLiteral("Route"));
    QCOMPARE(recovered.currentFontColor(), QStringLiteral("#ffeecc"));
    QVERIFY(recovered.dirty());
    recovered.discardRecovery();
    deck.discardSessionRecovery();
}

void PresentTests::templatesCreateWorkingDecks() {
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    PresentDocument deck;
    for (const QString &id : {QStringLiteral("trip-plan"), QStringLiteral("celebration"), QStringLiteral("story")}) {
        QVERIFY(deck.createTemplate(id));
        QCOMPARE(deck.slideCount(), 4);
        QCOMPARE(deck.selectedIndex(), 0);
        QVERIFY(deck.path().isEmpty());
        QVERIFY(deck.dirty());
        QVERIFY(!deck.currentBackgroundColor().isEmpty());
        deck.selectSlide(1);
        QVERIFY(!deck.currentTitle().isEmpty());
    }
    QVERIFY(!deck.createTemplate(QStringLiteral("unknown")));
    const QString path = dir.path() + QStringLiteral("/story.npresent");
    QVERIFY(deck.saveAs(QUrl::fromLocalFile(path)));
    PresentDocument reopened;
    QVERIFY(reopened.open(QUrl::fromLocalFile(path)));
    QCOMPARE(reopened.slideCount(), 4);
    QVERIFY(reopened.currentTitle().contains(QStringLiteral("story")));
}

void PresentTests::rejectsInvalidFileWithoutLosingSlides() {
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString path = dir.path() + QStringLiteral("/bad.npresent");
    QFile file(path);
    QVERIFY(file.open(QIODevice::WriteOnly));
    file.write("{\"format\":\"nexus-present\",\"version\":1,\"slides\":[{\"layout\":99}]}");
    file.close();
    PresentDocument deck;
    deck.newDocument();
    deck.setTitle(QStringLiteral("Keep this"));
    QVERIFY(!deck.open(QUrl::fromLocalFile(path)));
    QCOMPARE(deck.currentTitle(), QStringLiteral("Keep this"));
    QCOMPARE(deck.slideCount(), 1);
}

void PresentTests::exportsSlidesAsPdf() {
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    PresentDocument deck;
    deck.newDocument();
    deck.setTitle(QStringLiteral("Welcome home"));
    deck.setBody(QStringLiteral("A small presentation"));
    deck.addSlide();
    deck.setTitle(QStringLiteral("The next chapter"));
    deck.setBody(QStringLiteral("One step at a time"));
    deck.setStyle(1);
    deck.addSlide();
    deck.setTitle(QStringLiteral("Thank you"));
    deck.setStyle(2);
    const QString path = dir.path() + QStringLiteral("/slides.pdf");
    QVERIFY2(deck.exportPdf(QUrl::fromLocalFile(path), QColor(QStringLiteral("#7a88f0"))),
             qPrintable(deck.errorString()));
    QFile file(path);
    QVERIFY(file.open(QIODevice::ReadOnly));
    QVERIFY(file.read(5) == QByteArray("%PDF-"));
    QVERIFY(file.size() > 3000);
    QVERIFY(deck.dirty());
}

void PresentTests::pptxRoundTrip() {
    QTemporaryDir dir; QVERIFY(dir.isValid());
    PresentDocument deck; deck.newDocument();
    deck.setTitle("Welcome"); deck.setBody("First line\nSecond line");
    deck.setFontFamily("Noto Serif"); deck.setFontColor("#123456");
    deck.setBackgroundColor("#ABCDEF"); deck.setBold(true); deck.setTitleSize(42);
    deck.addSlide(); deck.setTitle("Next"); deck.setBody("The plan");
    const QString path=dir.path()+"/slides.pptx";
    QVERIFY2(deck.exportPptx(QUrl::fromLocalFile(path)),qPrintable(deck.errorString()));
    PresentDocument imported;
    QVERIFY2(imported.importPptx(QUrl::fromLocalFile(path)),qPrintable(imported.errorString()));
    QCOMPARE(imported.slideCount(),2);
    QCOMPARE(imported.currentTitle(),QString("Welcome"));
    QCOMPARE(imported.currentBody(),QString("First line\nSecond line"));
    QCOMPARE(imported.currentFontFamily(),QString("Noto Serif"));
    QCOMPARE(imported.currentFontColor(),QString("#123456"));
    QCOMPARE(imported.currentBackgroundColor(),QString("#ABCDEF"));
    QVERIFY(imported.currentBold()); QCOMPARE(imported.currentTitleSize(),42);
    imported.selectSlide(1); QCOMPARE(imported.currentTitle(),QString("Next"));
    QVERIFY(imported.path().isEmpty()); QVERIFY(imported.dirty());
    QFile bad(dir.path()+"/bad.pptx"); QVERIFY(bad.open(QIODevice::WriteOnly)); bad.write("invalid"); bad.close();
    QVERIFY(!imported.importPptx(QUrl::fromLocalFile(bad.fileName())));
    QCOMPARE(imported.currentTitle(),QString("Next"));
}

void PresentTests::slideObjectsRoundTrip() {
    QTemporaryDir dir;QVERIFY(dir.isValid());
    QImage image(240,120,QImage::Format_RGB32);image.fill(QColor("#d04a64"));
    const QString imagePath=dir.path()+"/photo.png";QVERIFY(image.save(imagePath));
    PresentDocument deck;deck.newDocument();deck.setTitle("Photo story");
    QVERIFY2(deck.addImage(QUrl::fromLocalFile(imagePath)),qPrintable(deck.errorString()));
    deck.addTextBox();QCOMPARE(deck.currentElements().size(),2);
    const QString imageId=deck.currentElements()[0].toMap().value("id").toString();
    const QString textId=deck.currentElements()[1].toMap().value("id").toString();
    const int originalX=deck.currentElements()[0].toMap().value("x").toInt();
    deck.setElementText(textId,"A summer memory");deck.setElementRect(imageId,60,80,420,210);
    QCOMPARE(deck.currentElements()[0].toMap().value("x").toInt(),60);
    deck.undo();QCOMPARE(deck.currentElements()[0].toMap().value("x").toInt(),originalX);
    deck.redo();QCOMPARE(deck.currentElements()[0].toMap().value("width").toInt(),420);
    QString native=dir.path()+"/objects.npresent";
    QVERIFY2(deck.saveAs(QUrl::fromLocalFile(native)),qPrintable(deck.errorString()));
    PresentDocument reopened;QVERIFY2(reopened.open(QUrl::fromLocalFile(native)),qPrintable(reopened.errorString()));
    QCOMPARE(reopened.currentElements().size(),2);
    QCOMPARE(reopened.currentElements()[1].toMap().value("text").toString(),QString("A summer memory"));
    QVERIFY(reopened.currentElements()[0].toMap().value("image").toString().startsWith("data:image/png;base64,"));
    QString pdf=dir.path()+"/objects.pdf";QVERIFY2(reopened.exportPdf(QUrl::fromLocalFile(pdf),QColor("#729ad6")),qPrintable(reopened.errorString()));
    QFile file(pdf);QVERIFY(file.open(QIODevice::ReadOnly));QCOMPARE(file.read(5),QByteArray("%PDF-"));
    QString pdftotext=QStandardPaths::findExecutable("pdftotext");
    if(!pdftotext.isEmpty()){QProcess process;process.start(pdftotext,{pdf,"-"});QVERIFY(process.waitForFinished());QVERIFY(QString::fromUtf8(process.readAllStandardOutput()).contains("A summer memory"));}
    QString pptx=dir.path()+"/objects.pptx";QVERIFY2(reopened.exportPptx(QUrl::fromLocalFile(pptx)),qPrintable(reopened.errorString()));
    PresentDocument imported;QVERIFY2(imported.importPptx(QUrl::fromLocalFile(pptx)),qPrintable(imported.errorString()));
    QCOMPARE(imported.currentElements().size(),2);
    QCOMPARE(imported.currentElements()[0].toMap().value("type").toString(),QString("image"));
    QCOMPARE(imported.currentElements()[1].toMap().value("text").toString(),QString("A summer memory"));
    reopened.removeElement(imageId);QCOMPARE(reopened.currentElements().size(),1);
    reopened.undo();QCOMPARE(reopened.currentElements().size(),2);
    reopened.setElementText(textId,"A summer memory recovered");
    reopened.writeRecoveryNow();PresentDocument recovered;QVERIFY(recovered.recoveryAvailable());
    QVERIFY(recovered.restoreRecovery());QCOMPARE(recovered.currentElements().size(),2);
    QCOMPARE(recovered.currentElements()[1].toMap().value("text").toString(),QString("A summer memory recovered"));
    recovered.discardRecovery();reopened.discardSessionRecovery();deck.discardSessionRecovery();
}

void PresentTests::shapesAndAlignmentRoundTrip() {
    QTemporaryDir dir;QVERIFY(dir.isValid());
    PresentDocument deck;deck.newDocument();
    deck.addShape("rectangle");deck.addShape("ellipse");
    QCOMPARE(deck.currentElements().size(),2);
    const QString id=deck.currentElements()[1].toMap().value("id").toString();
    deck.setElementFill(id,"#e05271");
    deck.alignElement(id,"right");deck.alignElement(id,"bottom");
    QVariantMap shape=deck.currentElements()[1].toMap();
    QCOMPARE(shape.value("shape").toString(),QString("ellipse"));
    QCOMPARE(shape.value("fill").toString(),QString("#e05271"));
    QCOMPARE(shape.value("x").toInt(),960-shape.value("width").toInt());
    QCOMPARE(shape.value("y").toInt(),540-shape.value("height").toInt());
    deck.undo();QCOMPARE(deck.currentElements()[1].toMap().value("y").toInt(),180);
    deck.redo();QCOMPARE(deck.currentElements()[1].toMap().value("y").toInt(),360);
    const QString native=dir.path()+"/shapes.npresent";
    QVERIFY2(deck.saveAs(QUrl::fromLocalFile(native)),qPrintable(deck.errorString()));
    PresentDocument opened;QVERIFY(opened.open(QUrl::fromLocalFile(native)));
    QCOMPARE(opened.currentElements().size(),2);
    QCOMPARE(opened.currentElements()[1].toMap().value("fill").toString(),QString("#e05271"));
    const QString pdf=dir.path()+"/shapes.pdf";QVERIFY(opened.exportPdf(QUrl::fromLocalFile(pdf),QColor("#5273cf")));
    const QString pptx=dir.path()+"/shapes.pptx";QVERIFY(opened.exportPptx(QUrl::fromLocalFile(pptx)));
    PresentDocument imported;QVERIFY2(imported.importPptx(QUrl::fromLocalFile(pptx)),qPrintable(imported.errorString()));
    QCOMPARE(imported.currentElements().size(),2);
    QCOMPARE(imported.currentElements()[1].toMap().value("shape").toString(),QString("ellipse"));
    QCOMPARE(imported.currentElements()[1].toMap().value("fill").toString(),QString("#e05271"));
    deck.discardSessionRecovery();opened.discardSessionRecovery();imported.discardSessionRecovery();
}

QTEST_MAIN(PresentTests)
#include "PresentTests.moc"
