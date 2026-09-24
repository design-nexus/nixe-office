#include "SheetsDocument.h"
#include "ThemeManager.h"
#include "IconProvider.h"

#include <QQmlComponent>
#include <QQmlContext>
#include <QQmlEngine>
#include <QQuickItem>
#include <QQuickWindow>
#include <QtTest>
#include <memory>

static QQuickItem *visibleCell(QQuickItem *parent, const QString &name) {
    if (parent->objectName() == name) return parent;
    for (QQuickItem *child : parent->childItems())
        if (auto *match = visibleCell(child, name)) return match;
    return nullptr;
}

class SheetsUiTests : public QObject {
    Q_OBJECT
private slots:
    void typingStartsOnlyAfterSelection();
    void mouseDragSelectsFillsAndMoves();
    void clickingCellsBuildsFormulaReferences();
};

void SheetsUiTests::typingStartsOnlyAfterSelection() {
    SheetsDocument sheets;
    sheets.newDocument();
    ThemeManager theme;
    QQmlEngine engine;
    engine.addImageProvider(QStringLiteral("nexus-icons"), new IconProvider);
    engine.rootContext()->setContextProperty(QStringLiteral("Sheets"), &sheets);
    engine.rootContext()->setContextProperty(QStringLiteral("Theme"), &theme);
    QQmlComponent component(&engine, QUrl::fromLocalFile(QStringLiteral(SHEETS_EDITOR_QML_PATH)));
    QVERIFY2(component.isReady(), qPrintable(component.errorString()));
    std::unique_ptr<QObject> object(component.create());
    QVERIFY2(object, qPrintable(component.errorString()));
    auto *editor = qobject_cast<QQuickItem *>(object.get());
    QVERIFY(editor);
    QQuickWindow window;
    window.resize(700, 500);
    editor->setParentItem(window.contentItem());
    editor->setSize(QSizeF(700, 500));
    window.show();

    QVERIFY(QMetaObject::invokeMethod(editor, "selectCell",
                                Q_ARG(QVariant, QVariant(0)), Q_ARG(QVariant, QVariant(0)),
                                Q_ARG(QVariant, QVariant(false))));
    QTRY_VERIFY(window.activeFocusItem() != nullptr);
    QVERIFY(!window.activeFocusItem()->inherits("QQuickTextInput"));
    QTest::keyClick(&window, Qt::Key_A, Qt::ShiftModifier);
    QTRY_VERIFY(window.activeFocusItem() && window.activeFocusItem()->inherits("QQuickTextInput"));
    QTest::keyClick(&window, Qt::Key_Return);
    QTRY_COMPARE(editor->property("selectedRow").toInt(), 1);
    QCOMPARE(editor->property("selectedColumn").toInt(), 0);
    QCOMPARE(sheets.rawAt(0, 0), QStringLiteral("a"));
    QTRY_VERIFY(window.activeFocusItem() && !window.activeFocusItem()->inherits("QQuickTextInput"));
    QTest::keyClick(&window, Qt::Key_B, Qt::ShiftModifier);
    QTest::keyClick(&window, Qt::Key_Return);
    QTRY_COMPARE(editor->property("selectedRow").toInt(), 2);
    QCOMPARE(sheets.rawAt(1, 0), QStringLiteral("b"));
}

void SheetsUiTests::mouseDragSelectsFillsAndMoves() {
    SheetsDocument sheets;
    sheets.newDocument();
    sheets.setCell(0, 0, "Seed");
    ThemeManager theme;
    QQmlEngine engine;
    engine.addImageProvider(QStringLiteral("nexus-icons"), new IconProvider);
    engine.rootContext()->setContextProperty(QStringLiteral("Sheets"), &sheets);
    engine.rootContext()->setContextProperty(QStringLiteral("Theme"), &theme);
    QQmlComponent component(&engine, QUrl::fromLocalFile(QStringLiteral(SHEETS_EDITOR_QML_PATH)));
    QVERIFY2(component.isReady(), qPrintable(component.errorString()));
    std::unique_ptr<QObject> object(component.create());
    QVERIFY2(object, qPrintable(component.errorString()));
    auto *editor = qobject_cast<QQuickItem *>(object.get());
    QVERIFY(editor);
    QQuickWindow window;
    window.resize(700, 500);
    editor->setParentItem(window.contentItem());
    editor->setSize(QSizeF(700, 500));
    window.show();
    QTest::qWait(50);
    auto *grid = editor->findChild<QQuickItem *>(QStringLiteral("sheetsGrid"));
    QVERIFY(grid);
    const qreal originalContentY = grid->property("contentY").toReal();

    QTest::mousePress(&window, Qt::LeftButton, Qt::NoModifier, QPoint(85, 44));
    for (int y = 50; y <= 103; y += 6) QTest::mouseMove(&window, QPoint(85, y), 10);
    QTest::mouseMove(&window, QPoint(85, 103), 10);
    QTest::mouseRelease(&window, Qt::LeftButton, Qt::NoModifier, QPoint(85, 103));
    QTRY_COMPARE(editor->property("anchorRow").toInt(), 0);
    QTRY_COMPARE(editor->property("selectedRow").toInt(), 2);
    QCOMPARE(editor->property("selectedColumn").toInt(), 0);
    QCOMPARE(grid->property("contentY").toReal(), originalContentY);

    // The bottom-right square of A3 is the fill handle.
    QTest::mousePress(&window, Qt::LeftButton, Qt::NoModifier, QPoint(152, 114));
    for (int y = 120; y <= 164; y += 5) QTest::mouseMove(&window, QPoint(152, y), 10);
    QTest::mouseMove(&window, QPoint(152, 164), 10);
    auto *fillPreview = visibleCell(editor, QStringLiteral("sheetCell-3-0"));
    QVERIFY(fillPreview);
    QTRY_COMPARE(fillPreview->property("shownText").toString(), QStringLiteral("Seed"));
    QCOMPARE(sheets.rawAt(3, 0), QString());
    QTest::mouseRelease(&window, Qt::LeftButton, Qt::NoModifier, QPoint(152, 164));
    QTRY_COMPARE(sheets.rawAt(3, 0), QStringLiteral("Seed"));
    QCOMPARE(editor->property("selectedRow").toInt(), 4);
    QCOMPARE(grid->property("contentY").toReal(), originalContentY);

    // The top-left grip moves the entire selected range.
    QTest::mousePress(&window, Qt::LeftButton, Qt::NoModifier, QPoint(48, 32));
    for (int step = 1; step <= 10; ++step)
        QTest::mouseMove(&window, QPoint(48 + step * 26, 32 + step * 10), 10);
    QTest::mouseMove(&window, QPoint(308, 134), 10);
    auto *movePreview = visibleCell(editor, QStringLiteral("sheetCell-3-2"));
    auto *sourcePreview = visibleCell(editor, QStringLiteral("sheetCell-0-0"));
    QVERIFY(movePreview);
    QVERIFY(sourcePreview);
    QTRY_COMPARE(movePreview->property("shownText").toString(), QStringLiteral("Seed"));
    QCOMPARE(sourcePreview->property("shownText").toString(), QString());
    QCOMPARE(sheets.rawAt(3, 2), QString());
    QCOMPARE(sheets.rawAt(0, 0), QStringLiteral("Seed"));
    QTest::mouseRelease(&window, Qt::LeftButton, Qt::NoModifier, QPoint(308, 134));
    QTRY_COMPARE(sheets.rawAt(3, 2), QStringLiteral("Seed"));
    QCOMPARE(sheets.rawAt(0, 0), QString());
    QCOMPARE(editor->property("anchorRow").toInt(), 3);
    QCOMPARE(editor->property("anchorColumn").toInt(), 2);
    QCOMPARE(grid->property("contentY").toReal(), originalContentY);
}

void SheetsUiTests::clickingCellsBuildsFormulaReferences() {
    SheetsDocument sheets;
    sheets.newDocument();
    sheets.setCell(0, 0, "2");
    sheets.setCell(1, 0, "3");
    sheets.setCell(2, 0, "4");
    ThemeManager theme;
    QQmlEngine engine;
    engine.addImageProvider(QStringLiteral("nexus-icons"), new IconProvider);
    engine.rootContext()->setContextProperty(QStringLiteral("Sheets"), &sheets);
    engine.rootContext()->setContextProperty(QStringLiteral("Theme"), &theme);
    QQmlComponent component(&engine, QUrl::fromLocalFile(QStringLiteral(SHEETS_EDITOR_QML_PATH)));
    QVERIFY2(component.isReady(), qPrintable(component.errorString()));
    std::unique_ptr<QObject> object(component.create());
    QVERIFY2(object, qPrintable(component.errorString()));
    auto *editor = qobject_cast<QQuickItem *>(object.get());
    QVERIFY(editor);
    QQuickWindow window;
    window.resize(700, 500);
    editor->setParentItem(window.contentItem());
    editor->setSize(QSizeF(700, 500));
    window.show();
    QTest::qWait(50);

    QVERIFY(QMetaObject::invokeMethod(editor, "selectCell",
                                    Q_ARG(QVariant, QVariant(0)), Q_ARG(QVariant, QVariant(2)),
                                    Q_ARG(QVariant, QVariant(false))));
    QVERIFY(QMetaObject::invokeMethod(editor, "beginSelectedEdit", Q_ARG(QVariant, QVariant("=SUM("))));
    QTRY_VERIFY(window.activeFocusItem() && window.activeFocusItem()->inherits("QQuickTextInput"));
    QTest::mousePress(&window, Qt::LeftButton, Qt::NoModifier, QPoint(85, 44));
    QTest::mouseMove(&window, QPoint(85, 103), 10);
    QTest::mouseRelease(&window, Qt::LeftButton, Qt::NoModifier, QPoint(85, 103));
    QTRY_COMPARE(window.activeFocusItem()->property("text").toString(), QStringLiteral("=SUM(A1:A3"));
    QCOMPARE(editor->property("selectedRow").toInt(), 0);
    QCOMPARE(editor->property("selectedColumn").toInt(), 2);
    window.activeFocusItem()->setProperty("text", "=SUM(A1:A3)");
    QTest::keyClick(&window, Qt::Key_Return);
    QCOMPARE(sheets.rawAt(0, 2), QStringLiteral("=SUM(A1:A3)"));
    QCOMPARE(sheets.displayAt(0, 2), QStringLiteral("9"));

    QVERIFY(QMetaObject::invokeMethod(editor, "selectCell",
                                    Q_ARG(QVariant, QVariant(1)), Q_ARG(QVariant, QVariant(2)),
                                    Q_ARG(QVariant, QVariant(false))));
    QVERIFY(QMetaObject::invokeMethod(editor, "beginSelectedEdit", Q_ARG(QVariant, QVariant("="))));
    QTest::mouseClick(&window, Qt::LeftButton, Qt::NoModifier, QPoint(196, 74));
    QTRY_COMPARE(window.activeFocusItem()->property("text").toString(), QStringLiteral("=B2"));
    QCOMPARE(editor->property("selectedRow").toInt(), 1);
    QCOMPARE(editor->property("selectedColumn").toInt(), 2);

    // The toolbar formula field uses the same reference picking behavior.
    QTest::keyClick(&window, Qt::Key_Escape);
    QCOMPARE(sheets.rawAt(1, 2), QString());
    QTRY_VERIFY(window.activeFocusItem() && !window.activeFocusItem()->inherits("QQuickTextInput"));
    QVERIFY(QMetaObject::invokeMethod(editor, "selectCell",
                                    Q_ARG(QVariant, QVariant(0)), Q_ARG(QVariant, QVariant(2)),
                                    Q_ARG(QVariant, QVariant(false))));
    QQmlComponent barComponent(&engine);
    barComponent.setData("import QtQuick\nTextInput {}", QUrl());
    QVERIFY2(barComponent.isReady(), qPrintable(barComponent.errorString()));
    std::unique_ptr<QObject> barObject(barComponent.create());
    auto *bar = qobject_cast<QQuickItem *>(barObject.get());
    QVERIFY(bar);
    bar->setParentItem(window.contentItem());
    bar->setWidth(250);
    bar->setHeight(24);
    bar->setProperty("text", "=SUM(");
    editor->setProperty("formulaBarEditor", QVariant::fromValue(static_cast<QObject *>(bar)));
    bar->forceActiveFocus();
    QTest::mouseClick(&window, Qt::LeftButton, Qt::NoModifier, QPoint(196, 44));
    QTRY_COMPARE(bar->property("text").toString(), QStringLiteral("=SUM(B1"));
    QVERIFY(bar->hasActiveFocus());
    QCOMPARE(editor->property("selectedRow").toInt(), 0);
    QCOMPARE(editor->property("selectedColumn").toInt(), 2);
}

QTEST_MAIN(SheetsUiTests)
#include "SheetsUiTests.moc"
