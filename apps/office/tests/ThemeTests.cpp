#include "ThemeManager.h"

#include <QtTest>
#include <QTemporaryDir>
#include <QCoreApplication>

class ThemeTests : public QObject {
    Q_OBJECT

private slots:
    void parsesDarkOmarchyPalette();
    void parsesLightOmarchyPalette();
    void rejectsInvalidPalette();
    void loadsAndSelectsBuiltInThemes();
};

void ThemeTests::parsesDarkOmarchyPalette() {
    const auto palette = ThemeManager::parseOmarchyPalette(R"(
mode = "dark"
background = "#1a1b26"
dark_background = "#13141c"
lighter_background = "#24283b"
foreground = "#a9b1d6"
accent = "#7aa2f7"
selection = "#414868"
red = "#f7768e"
)");
    QCOMPARE(palette.value("background").toString(), QStringLiteral("#1a1b26"));
    QCOMPARE(palette.value("surface").toString(), QStringLiteral("#13141c"));
    QCOMPARE(palette.value("accent").toString(), QStringLiteral("#7aa2f7"));
    QCOMPARE(palette.value("dark").toBool(), true);
}

void ThemeTests::parsesLightOmarchyPalette() {
    const auto palette = ThemeManager::parseOmarchyPalette(R"(
mode = "light"
background = "#faf4ed"
foreground = "#575279"
accent = "#56949f"
)");
    QCOMPARE(palette.value("dark").toBool(), false);
    QCOMPARE(palette.value("surface").toString(), QStringLiteral("#faf4ed"));
    QCOMPARE(palette.value("text").toString(), QStringLiteral("#575279"));
}

void ThemeTests::rejectsInvalidPalette() {
    QVERIFY(ThemeManager::parseOmarchyPalette("background = \"oops\"\nforeground = \"#ffffff\"").isEmpty());
    QVERIFY(ThemeManager::parseOmarchyPalette("background = \"#000000\"").isEmpty());
}

void ThemeTests::loadsAndSelectsBuiltInThemes() {
    ThemeManager theme;
    QCOMPARE(theme.themes().size(), 10);
    theme.setSelectedTheme(QStringLiteral("dracula"));
    QCOMPARE(theme.selectedTheme(), QStringLiteral("dracula"));
    QCOMPARE(theme.activeThemeName(), QStringLiteral("Dracula"));
    QCOMPARE(theme.palette().value("background").toString(), QStringLiteral("#282a36"));
    theme.setSelectedTheme(QStringLiteral("nonexistent"));
    QCOMPARE(theme.selectedTheme(), QStringLiteral("dracula"));
}

int main(int argc, char **argv) {
    QTemporaryDir settingsDir;
    if (!settingsDir.isValid()) return 1;
    qputenv("XDG_CONFIG_HOME", settingsDir.path().toUtf8());
    QCoreApplication app(argc, argv);
    ThemeTests tests;
    return QTest::qExec(&tests, argc, argv);
}
#include "ThemeTests.moc"
