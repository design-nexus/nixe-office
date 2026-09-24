#include "SheetsDocument.h"

#include <QFile>
#include <QClipboard>
#include <QGuiApplication>
#include <QTemporaryDir>
#include <QtTest>

class SheetsTests : public QObject {
    Q_OBJECT
private slots:
    void calculatesReferencesAndRanges();
    void handlesFormulaErrors();
    void csvRoundTrip();
    void keepsSheetWhenCsvIsInvalid();
    void nativeFormattingSortFilterChartAndPdf();
    void dimensionsRoundTrip();
    void undoRedoAndRecovery();
    void templatesCreateWorkingSheets();
    void xlsxRoundTrip();
    void multiSheetAndRangeClipboard();
    void rangeFillMoveAndPaste();
    void expandedFormulasAndCharts();
    void conditionalFormulasAndRangeStyles();
};

void SheetsTests::calculatesReferencesAndRanges() {
    SheetsDocument sheet;
    sheet.newDocument();
    sheet.setCell(0, 0, QStringLiteral("12"));
    sheet.setCell(1, 0, QStringLiteral("8"));
    sheet.setCell(0, 1, QStringLiteral("5"));
    sheet.setCell(1, 1, QStringLiteral("3"));
    sheet.setCell(0, 2, QStringLiteral("=A1+A2*2"));
    QCOMPARE(sheet.displayAt(0, 2), QStringLiteral("28"));
    sheet.setCell(1, 2, QStringLiteral("=SUM(A1:B2)"));
    QCOMPARE(sheet.displayAt(1, 2), QStringLiteral("28"));
    sheet.setCell(2, 2, QStringLiteral("=AVERAGE(A1,A2)"));
    QCOMPARE(sheet.displayAt(2, 2), QStringLiteral("10"));
    sheet.setCell(3, 2, QStringLiteral("=MAX(A1:B2)-MIN(A1:B2)"));
    QCOMPARE(sheet.displayAt(3, 2), QStringLiteral("9"));
    sheet.setCell(0, 0, QStringLiteral("20"));
    QCOMPARE(sheet.displayAt(0, 2), QStringLiteral("36"));
    QCOMPARE(sheet.displayAt(1, 2), QStringLiteral("36"));
    QCOMPARE(sheet.cellName(0, 26), QStringLiteral("AA1"));
    QVERIFY(sheet.dirty());
}

void SheetsTests::expandedFormulasAndCharts() {
    QTemporaryDir dir;QVERIFY(dir.isValid());
    SheetsDocument sheet;sheet.newDocument();
    sheet.setCell(0,0,"Item");sheet.setCell(1,0,"Rent");sheet.setCell(2,0,"Food");
    sheet.setCell(1,1,"12");sheet.setCell(2,1,"8");sheet.setCell(3,1,"Note");
    const QList<QPair<QString,QString>> cases={
        {"=COUNT(B2:B5)","2"},{"=COUNTA(B2:B5)","3"},{"=AVERAGE(B2:B5)","10"},
        {"=SUM(B2:B5)","20"},{"=PRODUCT(B2:B3)","96"},{"=MEDIAN(B2:B3)","10"},
        {"=ABS(-9)","9"},{"=ROUND(12.345,2)","12.35"},{"=SQRT(81)","9"},
        {"=POWER(2,5)","32"},{"=SQRT(-1)","#NUM!"}};
    for(const auto &test:cases){sheet.setCell(5,2,test.first);QCOMPARE(sheet.displayAt(5,2),test.second);}
    sheet.setChart("A1:B3","area");QCOMPARE(sheet.chartType(),QString("area"));
    const QString native=dir.path()+"/charts.nsheets";QVERIFY(sheet.saveAs(QUrl::fromLocalFile(native)));
    SheetsDocument opened;QVERIFY(opened.open(QUrl::fromLocalFile(native)));
    QCOMPARE(opened.chartType(),QString("area"));QCOMPARE(opened.chartData().size(),2);
    opened.setChart("A1:B3","pie");QCOMPARE(opened.chartType(),QString("pie"));
    const QString pdf=dir.path()+"/chart.pdf";QVERIFY2(opened.exportPdf(QUrl::fromLocalFile(pdf)),qPrintable(opened.errorString()));
    QFile file(pdf);QVERIFY(file.open(QIODevice::ReadOnly));QCOMPARE(file.read(5),QByteArray("%PDF-"));
    opened.setChart("A1:B3","unknown");QCOMPARE(opened.chartType(),QString("pie"));
    opened.undo();QCOMPARE(opened.chartType(),QString("area"));
    opened.redo();QCOMPARE(opened.chartType(),QString("pie"));
    sheet.discardSessionRecovery();opened.discardSessionRecovery();
}

void SheetsTests::handlesFormulaErrors() {
    SheetsDocument sheet;
    sheet.newDocument();
    sheet.setCell(0, 0, QStringLiteral("=B1"));
    sheet.setCell(0, 1, QStringLiteral("=A1"));
    QCOMPARE(sheet.displayAt(0, 0), QStringLiteral("#CYCLE!"));
    sheet.setCell(0, 1, QStringLiteral("=1/0"));
    QCOMPARE(sheet.displayAt(0, 0), QStringLiteral("#DIV/0!"));
    sheet.setCell(0, 1, QStringLiteral("=AZ501"));
    QCOMPARE(sheet.displayAt(0, 1), QStringLiteral("#REF!"));
    sheet.setCell(0, 1, QStringLiteral("=NOTAFUNCTION(2)"));
    QCOMPARE(sheet.displayAt(0, 1), QStringLiteral("#NAME?"));
}

void SheetsTests::csvRoundTrip() {
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString path = dir.path() + QStringLiteral("/budget.csv");
    SheetsDocument sheet;
    sheet.newDocument();
    sheet.setCell(0, 0, QStringLiteral("Item"));
    sheet.setCell(0, 1, QStringLiteral("Amount"));
    sheet.setCell(1, 0, QStringLiteral("Rent, utilities"));
    sheet.setCell(1, 1, QStringLiteral("1200"));
    sheet.setCell(2, 0, QStringLiteral("He said \"hello\"\nagain"));
    sheet.setCell(2, 1, QStringLiteral("=B2*2"));
    QVERIFY2(sheet.saveAs(QUrl::fromLocalFile(path)), qPrintable(sheet.errorString()));
    QVERIFY(!sheet.dirty());

    SheetsDocument reopened;
    QVERIFY2(reopened.open(QUrl::fromLocalFile(path)), qPrintable(reopened.errorString()));
    QCOMPARE(reopened.rawAt(1, 0), QStringLiteral("Rent, utilities"));
    QCOMPARE(reopened.rawAt(2, 0), QStringLiteral("He said \"hello\"\nagain"));
    QCOMPARE(reopened.rawAt(2, 1), QStringLiteral("=B2*2"));
    QCOMPARE(reopened.displayAt(2, 1), QStringLiteral("2400"));
    QVERIFY(!reopened.dirty());
}

void SheetsTests::keepsSheetWhenCsvIsInvalid() {
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString path = dir.path() + QStringLiteral("/bad.csv");
    QFile file(path);
    QVERIFY(file.open(QIODevice::WriteOnly));
    file.write("\"unterminated");
    file.close();
    SheetsDocument sheet;
    sheet.newDocument();
    sheet.setCell(0, 0, QStringLiteral("Keep me"));
    QVERIFY(!sheet.open(QUrl::fromLocalFile(path)));
    QCOMPARE(sheet.rawAt(0, 0), QStringLiteral("Keep me"));
}

void SheetsTests::dimensionsRoundTrip() {
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    SheetsDocument sheet;
    sheet.newDocument();
    sheet.setColumnWidth(2, 210);
    sheet.setRowHeight(3, 54);
    sheet.setColumnWidth(4, 999);
    QCOMPARE(sheet.columnWidth(4), 480);
    QCOMPARE(sheet.rowHeight(3), 54);
    QVERIFY(sheet.dirty());
    const QString path = dir.path() + QStringLiteral("/sizes.nsheets");
    QVERIFY(sheet.saveAs(QUrl::fromLocalFile(path)));
    SheetsDocument reopened;
    QVERIFY(reopened.open(QUrl::fromLocalFile(path)));
    QCOMPARE(reopened.columnWidth(2), 210);
    QCOMPARE(reopened.rowHeight(3), 54);
    QCOMPARE(reopened.columnWidth(1), 112);
    QCOMPARE(reopened.rowHeight(1), 30);
    QVERIFY(!reopened.dirty());
}

void SheetsTests::undoRedoAndRecovery() {
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    SheetsDocument sheet;
    sheet.newDocument();
    sheet.setCell(0, 0, QStringLiteral("Rent"));
    sheet.setColumnWidth(0, 180);
    sheet.setNumberFormat(0, 0, 2);
    const QString path = dir.path() + QStringLiteral("/history.nsheets");
    QVERIFY(sheet.saveAs(QUrl::fromLocalFile(path)));
    sheet.setCell(0, 0, QStringLiteral("Mortgage"));
    sheet.beginDimensionResize();
    sheet.setRowHeight(0, 41);
    sheet.setRowHeight(0, 52);
    sheet.endDimensionResize();
    QVERIFY(sheet.canUndo());
    sheet.undo();
    QCOMPARE(sheet.rowHeight(0), 30);
    sheet.undo();
    QCOMPARE(sheet.rawAt(0, 0), QStringLiteral("Rent"));
    QVERIFY(!sheet.dirty());
    sheet.redo();
    QCOMPARE(sheet.rawAt(0, 0), QStringLiteral("Mortgage"));
    QVERIFY(sheet.dirty());
    sheet.setColumnWidth(0, 205);
    QVERIFY(!sheet.canRedo());
    sheet.writeRecoveryNow();
    SheetsDocument recovered;
    QVERIFY(recovered.recoveryAvailable());
    QVERIFY(recovered.restoreRecovery());
    QCOMPARE(recovered.path(), path);
    QCOMPARE(recovered.rawAt(0, 0), QStringLiteral("Mortgage"));
    QCOMPARE(recovered.columnWidth(0), 205);
    QCOMPARE(recovered.numberFormatAt(0, 0), 2);
    QVERIFY(recovered.dirty());
    recovered.discardRecovery();
    sheet.discardSessionRecovery();
}

void SheetsTests::templatesCreateWorkingSheets() {
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    SheetsDocument sheet;
    QVERIFY(sheet.createTemplate(QStringLiteral("monthly-budget")));
    QVERIFY(sheet.path().isEmpty());
    QVERIFY(sheet.dirty());
    sheet.setCell(1, 1, QStringLiteral("1000"));
    sheet.setCell(1, 2, QStringLiteral("900"));
    QCOMPARE(sheet.displayAt(1, 3), QStringLiteral("$100.00"));
    QCOMPARE(sheet.displayAt(9, 3), QStringLiteral("$100.00"));
    const QString path = dir.path() + QStringLiteral("/budget.nsheets");
    QVERIFY(sheet.saveAs(QUrl::fromLocalFile(path)));
    SheetsDocument reopened;
    QVERIFY(reopened.open(QUrl::fromLocalFile(path)));
    QCOMPARE(reopened.displayAt(9, 3), QStringLiteral("$100.00"));
    QVERIFY(sheet.createTemplate(QStringLiteral("home-inventory")));
    QCOMPARE(sheet.displayAt(5, 4), QStringLiteral("$0.00"));
    QVERIFY(sheet.createTemplate(QStringLiteral("trip-budget")));
    QCOMPARE(sheet.rawAt(7, 3), QStringLiteral("=SUM(D2:D7)"));
    QVERIFY(!sheet.createTemplate(QStringLiteral("unknown")));
    QCOMPARE(sheet.rawAt(7, 3), QStringLiteral("=SUM(D2:D7)"));
}

void SheetsTests::nativeFormattingSortFilterChartAndPdf() {
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    SheetsDocument sheet;
    sheet.newDocument();
    sheet.setCell(0, 0, QStringLiteral("Item"));
    sheet.setCell(0, 1, QStringLiteral("Cost"));
    sheet.setCell(1, 0, QStringLiteral("Rent"));
    sheet.setCell(1, 1, QStringLiteral("1200"));
    sheet.setCell(2, 0, QStringLiteral("Food"));
    sheet.setCell(2, 1, QStringLiteral("350"));
    sheet.setNumberFormat(1, 1, 2);
    sheet.toggleBold(0, 0);
    QCOMPARE(sheet.displayAt(1, 1), QStringLiteral("$1200.00"));
    QVERIFY(sheet.boldAt(0, 0));
    sheet.sortBy(1, false);
    QCOMPARE(sheet.sourceRow(0), 0);
    QCOMPARE(sheet.sourceRow(1), 1);
    sheet.setFilter(0, QStringLiteral("food"));
    QCOMPARE(sheet.visibleRowCount(), 1);
    QCOMPARE(sheet.sourceRow(0), 2);
    sheet.setFilter(0, {});
    sheet.clearSort();
    sheet.setChart(QStringLiteral("A1:B3"), QStringLiteral("bar"));
    QCOMPARE(sheet.chartData().size(), 2);
    sheet.sortBy(1, false);
    sheet.setFilter(0, QStringLiteral("food"));

    const QString csvPath = dir.path() + QStringLiteral("/lost.csv");
    QVERIFY(!sheet.saveAs(QUrl::fromLocalFile(csvPath)));
    const QString nativePath = dir.path() + QStringLiteral("/budget.nsheets");
    QVERIFY2(sheet.saveAs(QUrl::fromLocalFile(nativePath)), qPrintable(sheet.errorString()));
    QVERIFY(!sheet.dirty());
    QVERIFY2(sheet.exportCsv(QUrl::fromLocalFile(csvPath)), qPrintable(sheet.errorString()));
    QCOMPARE(sheet.path(), nativePath);
    SheetsDocument csvReader;
    QVERIFY(csvReader.open(QUrl::fromLocalFile(csvPath)));
    QCOMPARE(csvReader.rawAt(1, 1), QStringLiteral("1200"));
    QCOMPARE(csvReader.numberFormatAt(1, 1), 0);

    SheetsDocument reopened;
    QVERIFY2(reopened.open(QUrl::fromLocalFile(nativePath)), qPrintable(reopened.errorString()));
    QCOMPARE(reopened.rawAt(1, 1), QStringLiteral("1200"));
    QCOMPARE(reopened.displayAt(1, 1), QStringLiteral("$1200.00"));
    QVERIFY(reopened.boldAt(0, 0));
    QCOMPARE(reopened.chartRange(), QStringLiteral("A1:B3"));
    QCOMPARE(reopened.chartData().size(), 2);
    QCOMPARE(reopened.visibleRowCount(), 1);
    QCOMPARE(reopened.sourceRow(0), 2);
    QVERIFY(!reopened.dirty());
    reopened.setFilter(0, {});
    reopened.clearSort();

    const QString pdfPath = dir.path() + QStringLiteral("/budget.pdf");
    QVERIFY2(reopened.exportPdf(QUrl::fromLocalFile(pdfPath)), qPrintable(reopened.errorString()));
    QFile pdf(pdfPath);
    QVERIFY(pdf.open(QIODevice::ReadOnly));
    QCOMPARE(pdf.read(5), QByteArray("%PDF-"));
}

void SheetsTests::xlsxRoundTrip() {
    QTemporaryDir dir; QVERIFY(dir.isValid());
    SheetsDocument sheet; sheet.newDocument();
    sheet.setCell(0, 0, "Item"); sheet.setCell(1, 0, "Rent");
    sheet.setCell(1, 1, "1200"); sheet.setCell(2, 1, "=B2*2");
    sheet.toggleBold(0, 0); sheet.setNumberFormat(1, 1, 2);
    sheet.setRangeFillColor(0, 0, 0, 0, "#224466");
    sheet.setRangeTextColor(0, 0, 0, 0, "#f0e0d0");
    sheet.setRangeAlignment(0, 0, 0, 0, 1);
    sheet.setRangeFillColor(4, 4, 4, 4, "#aabbcc");
    sheet.setColumnWidth(1, 210); sheet.setRowHeight(1, 40);
    const QString path=dir.path()+"/budget.xlsx";
    QVERIFY2(sheet.exportXlsx(QUrl::fromLocalFile(path)), qPrintable(sheet.errorString()));
    QVERIFY(sheet.path().isEmpty()); QVERIFY(sheet.dirty());
    SheetsDocument imported;
    QVERIFY2(imported.importXlsx(QUrl::fromLocalFile(path)), qPrintable(imported.errorString()));
    QCOMPARE(imported.rawAt(0, 0), QString("Item"));
    QCOMPARE(imported.rawAt(1, 1), QString("1200"));
    QCOMPARE(imported.rawAt(2, 1), QString("=B2*2"));
    QCOMPARE(imported.displayAt(2, 1), QString("2400"));
    QCOMPARE(imported.numberFormatAt(1, 1), 2);
    QVERIFY(imported.boldAt(0, 0));
    QCOMPARE(imported.fillColorAt(0, 0), QString("#224466"));
    QCOMPARE(imported.textColorAt(0, 0), QString("#f0e0d0"));
    QCOMPARE(imported.alignmentAt(0, 0), 1);
    QCOMPARE(imported.fillColorAt(4, 4), QString("#aabbcc"));
    QCOMPARE(imported.columnWidth(1), 210);
    QCOMPARE(imported.rowHeight(1), 40);
    QVERIFY(imported.path().isEmpty()); QVERIFY(imported.dirty());
    imported.setCell(0, 0, "Keep");
    QFile bad(dir.path()+"/bad.xlsx"); QVERIFY(bad.open(QIODevice::WriteOnly)); bad.write("invalid"); bad.close();
    QVERIFY(!imported.importXlsx(QUrl::fromLocalFile(bad.fileName())));
    QCOMPARE(imported.rawAt(0, 0), QString("Keep"));
}

void SheetsTests::multiSheetAndRangeClipboard() {
    QTemporaryDir dir; QVERIFY(dir.isValid());
    SheetsDocument book; book.newDocument();
    book.setCell(0,0,"Home");book.setCell(0,1,"42");
    book.copyRange(0,0,0,1);QCOMPARE(QGuiApplication::clipboard()->text(),QString("Home\t42"));
    book.addSheet();QCOMPARE(book.sheetNames().size(),2);QCOMPARE(book.activeSheetIndex(),1);
    book.renameSheet(1,"Expenses");QCOMPARE(book.sheetNames()[1],QString("Expenses"));
    book.pasteRange(3,2);QCOMPARE(book.rawAt(3,2),QString("Home"));QCOMPARE(book.rawAt(3,3),QString("42"));
    book.setCell(1,0,"=B1*2");book.copyRange(1,0,1,0);book.pasteRange(2,2);
    QCOMPARE(book.rawAt(2,2),QString("=D2*2"));
    book.setCell(0,0,"Second");book.selectSheet(0);QCOMPARE(book.rawAt(0,0),QString("Home"));
    book.selectSheet(1);QCOMPARE(book.rawAt(0,0),QString("Second"));
    book.duplicateSheet();QCOMPARE(book.sheetNames().size(),3);QCOMPARE(book.rawAt(0,0),QString("Second"));
    book.removeSheet(2);QCOMPARE(book.sheetNames().size(),2);
    book.undo();QCOMPARE(book.sheetNames().size(),3);book.redo();QCOMPARE(book.sheetNames().size(),2);
    QString native=dir.path()+"/tabs.nsheets";
    QVERIFY2(book.saveAs(QUrl::fromLocalFile(native)),qPrintable(book.errorString()));
    SheetsDocument restored;QVERIFY2(restored.open(QUrl::fromLocalFile(native)),qPrintable(restored.errorString()));
    QCOMPARE(restored.sheetNames(),(QStringList{"Sheet 1","Expenses"}));QCOMPARE(restored.activeSheetIndex(),1);
    QCOMPARE(restored.rawAt(3,2),QString("Home"));restored.selectSheet(0);QCOMPARE(restored.rawAt(0,0),QString("Home"));
    QString excel=dir.path()+"/tabs.xlsx";QVERIFY2(restored.exportXlsx(QUrl::fromLocalFile(excel)),qPrintable(restored.errorString()));
    SheetsDocument imported;QVERIFY2(imported.importXlsx(QUrl::fromLocalFile(excel)),qPrintable(imported.errorString()));
    QCOMPARE(imported.sheetNames(),(QStringList{"Sheet 1","Expenses"}));
    QCOMPARE(imported.rawAt(0,0),QString("Home"));imported.selectSheet(1);QCOMPARE(imported.rawAt(3,2),QString("Home"));
    QGuiApplication::clipboard()->setText("one\ttwo\nthree\tfour");imported.pasteRange(5,4);
    QCOMPARE(imported.rawAt(5,4),QString("one"));QCOMPARE(imported.rawAt(6,5),QString("four"));
    imported.undo();QCOMPARE(imported.rawAt(5,4),QString());
}

void SheetsTests::rangeFillMoveAndPaste() {
    SheetsDocument book; book.newDocument();
    book.setCell(0, 0, "Seed");
    book.setCell(0, 1, "=A1");
    book.toggleBold(0, 0);
    book.fillRange(0, 0, 0, 1, 2, 1);
    QCOMPARE(book.rawAt(1, 0), QString("Seed"));
    QCOMPARE(book.rawAt(2, 1), QString("=A3"));
    QVERIFY(book.boldAt(2, 0));
    book.undo();
    QCOMPARE(book.rawAt(1, 0), QString());
    book.redo();
    QCOMPARE(book.rawAt(2, 1), QString("=A3"));

    book.moveRange(0, 0, 2, 1, 3, 2);
    QCOMPARE(book.rawAt(0, 0), QString());
    QCOMPARE(book.rawAt(3, 2), QString("Seed"));
    QCOMPARE(book.rawAt(5, 3), QString("=A3"));
    QVERIFY(book.boldAt(5, 2));
    book.undo();
    QCOMPARE(book.rawAt(0, 0), QString("Seed"));

    book.copyRange(0, 0, 0, 0);
    book.pasteRangeToSelection(5, 5, 6, 6);
    QCOMPARE(book.rawAt(5, 5), QString("Seed"));
    QCOMPARE(book.rawAt(6, 6), QString("Seed"));
    book.clearRange(5, 5, 6, 6);
    QCOMPARE(book.rawAt(5, 5), QString());
    QCOMPARE(book.rawAt(6, 6), QString());
    book.undo();
    QCOMPARE(book.rawAt(6, 6), QString("Seed"));

    book.setCell(8, 8, "=A1");
    book.fillRange(8, 8, 8, 8, 8, 9);
    QCOMPARE(book.rawAt(8, 9), QString("=B1"));
    book.copyRange(8, 8, 8, 8);
    book.pasteRangeToSelection(9, 8, 10, 8);
    QCOMPARE(book.rawAt(9, 8), QString("=A2"));
    QCOMPARE(book.rawAt(10, 8), QString("=A3"));

    book.setCell(15, 0, "Top");
    book.setCell(16, 0, "Bottom");
    book.moveRange(15, 0, 16, 0, 16, 0);
    QCOMPARE(book.rawAt(15, 0), QString());
    QCOMPARE(book.rawAt(16, 0), QString("Top"));
    QCOMPARE(book.rawAt(17, 0), QString("Bottom"));
}

void SheetsTests::conditionalFormulasAndRangeStyles() {
    QTemporaryDir dir; QVERIFY(dir.isValid());
    SheetsDocument sheet; sheet.newDocument();
    sheet.setCell(0, 0, "Food"); sheet.setCell(0, 1, "12");
    sheet.setCell(1, 0, "Travel"); sheet.setCell(1, 1, "8");
    sheet.setCell(2, 0, "Food"); sheet.setCell(2, 1, "5");
    sheet.setCell(0, 2, "=IF(B1>10,\"High\",\"Low\")");
    QCOMPARE(sheet.displayAt(0, 2), QString("High"));
    sheet.setCell(1, 2, "=IF(B2>10,1/0,42)");
    QCOMPARE(sheet.displayAt(1, 2), QString("42"));
    sheet.setCell(2, 2, "=COUNTIF(A1:A3,\"Food\")");
    QCOMPARE(sheet.displayAt(2, 2), QString("2"));
    sheet.setCell(3, 2, "=SUMIF(A1:A3,\"Food\",B1:B3)");
    QCOMPARE(sheet.displayAt(3, 2), QString("17"));
    sheet.setCell(4, 2, "=COUNTIF(B1:B3,\">7\")");
    QCOMPARE(sheet.displayAt(4, 2), QString("2"));
    sheet.setCell(5, 2, "=AND(B1>10,B2<10)");
    QCOMPARE(sheet.displayAt(5, 2), QString("1"));
    sheet.setCell(6, 2, "=MOD(B1,5)+INT(2.9)");
    QCOMPARE(sheet.displayAt(6, 2), QString("4"));

    sheet.setRangeFillColor(0, 0, 1, 1, "#224466");
    sheet.setRangeTextColor(0, 0, 1, 1, "#f0e0d0");
    sheet.setRangeAlignment(0, 0, 1, 1, 1);
    sheet.setRangeNumberFormat(0, 1, 1, 1, 2);
    sheet.toggleRangeBold(0, 0, 1, 1);
    QCOMPARE(sheet.fillColorAt(1, 1), QString("#224466"));
    QCOMPARE(sheet.textColorAt(0, 0), QString("#f0e0d0"));
    QCOMPARE(sheet.alignmentAt(1, 0), 1);
    QCOMPARE(sheet.displayAt(1, 1), QString("$8.00"));
    QVERIFY(sheet.boldAt(1, 1));
    sheet.undo();
    QVERIFY(!sheet.boldAt(1, 1));
    sheet.redo();
    QVERIFY(sheet.boldAt(1, 1));

    const QString native = dir.path() + "/styled.nsheets";
    QVERIFY(sheet.saveAs(QUrl::fromLocalFile(native)));
    SheetsDocument restored; QVERIFY(restored.open(QUrl::fromLocalFile(native)));
    QCOMPARE(restored.fillColorAt(1, 1), QString("#224466"));
    QCOMPARE(restored.textColorAt(0, 0), QString("#f0e0d0"));
    QCOMPARE(restored.alignmentAt(1, 0), 1);
    QCOMPARE(restored.displayAt(3, 2), QString("17"));
}

QTEST_MAIN(SheetsTests)
#include "SheetsTests.moc"
