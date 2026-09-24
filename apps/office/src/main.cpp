#include "AppController.h"
#include "ThemeManager.h"
#include "WriteDocument.h"
#include "SheetsDocument.h"
#include "PresentDocument.h"
#include "IconProvider.h"

#include <QCommandLineParser>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickWindow>
#include <QTimer>
#include <cstdio>

int main(int argc, char *argv[]) {
    QGuiApplication app(argc, argv);
    QCoreApplication::setOrganizationName(QStringLiteral("Nexus"));
    QCoreApplication::setApplicationName(QStringLiteral("Nexus Office Suite"));
    QCoreApplication::setApplicationVersion(QStringLiteral(NEXUS_VERSION));

    QCommandLineParser parser;
    parser.setApplicationDescription(QStringLiteral("Nexus Office Suite"));
    parser.addHelpOption();
    parser.addVersionOption();
    parser.addOption({QStringLiteral("app"), QStringLiteral("Open write, sheets, or present."),
                      QStringLiteral("name"), QStringLiteral("write")});
    parser.addOption({QStringLiteral("smoke"), QStringLiteral("Exit shortly after the QML window loads.")});
    parser.addOption({QStringLiteral("screenshot"), QStringLiteral("Save a screenshot and exit."),
                      QStringLiteral("path")});
    parser.addOption({QStringLiteral("new-document"), QStringLiteral("Start with a blank document in the selected app.")});
    parser.addOption({QStringLiteral("export-pdf"), QStringLiteral("Export a Nexus Present file as PDF and exit."),
                      QStringLiteral("path")});
    parser.addOption({QStringLiteral("preview-file-menu"), QStringLiteral("Open the File menu for a visual check.")});
    parser.addOption({QStringLiteral("preview-format-menu"), QStringLiteral("Open the Format menu for a visual check.")});
    parser.addOption({QStringLiteral("preview-slideshow"), QStringLiteral("Open the slideshow for a visual check.")});
    parser.addOption({QStringLiteral("preview-template-picker"), QStringLiteral("Open the template picker for a visual check.")});
    parser.addPositionalArgument(QStringLiteral("file"), QStringLiteral("A document supported by the selected app."));
    parser.process(app);

    QString appId = parser.value(QStringLiteral("app"));
    if (appId != QStringLiteral("write") && appId != QStringLiteral("sheets") && appId != QStringLiteral("present")) {
        std::fprintf(stderr, "Unknown app. Use write, sheets, or present.\n");
        return 2;
    }

    ThemeManager theme;
    AppController controller(appId);
    WriteDocument writeDocument;
    SheetsDocument sheetsDocument;
    PresentDocument presentDocument;
    if (parser.isSet(QStringLiteral("export-pdf"))) {
        if (appId != QStringLiteral("present") || parser.positionalArguments().isEmpty()) {
            std::fprintf(stderr, "PDF export requires --app present and a .npresent file.\n");
            return 2;
        }
        const QColor accent(theme.palette().value(QStringLiteral("accent")).toString());
        if (!presentDocument.openPath(parser.positionalArguments().first()) ||
            !presentDocument.exportPdf(QUrl::fromLocalFile(parser.value(QStringLiteral("export-pdf"))), accent)) {
            std::fprintf(stderr, "%s\n", qPrintable(presentDocument.errorString()));
            return 1;
        }
        return 0;
    }
    QQmlApplicationEngine engine;
    engine.addImageProvider(QStringLiteral("nexus-icons"), new IconProvider());
    QObject::connect(&engine, &QQmlApplicationEngine::warnings, &app, [](const QList<QQmlError> &warnings) {
        for (const QQmlError &warning : warnings)
            std::fprintf(stderr, "%s\n", qPrintable(warning.toString()));
    });
    engine.rootContext()->setContextProperty(QStringLiteral("Theme"), &theme);
    engine.rootContext()->setContextProperty(QStringLiteral("App"), &controller);
    engine.rootContext()->setContextProperty(QStringLiteral("Write"), &writeDocument);
    engine.rootContext()->setContextProperty(QStringLiteral("Sheets"), &sheetsDocument);
    engine.rootContext()->setContextProperty(QStringLiteral("Present"), &presentDocument);
    engine.rootContext()->setContextProperty(QStringLiteral("startBlankDocument"), parser.isSet(QStringLiteral("new-document")));
    engine.rootContext()->setContextProperty(QStringLiteral("initialDocumentPath"), parser.positionalArguments().value(0));
    engine.rootContext()->setContextProperty(QStringLiteral("previewFileMenu"), parser.isSet(QStringLiteral("preview-file-menu")));
    engine.rootContext()->setContextProperty(QStringLiteral("previewFormatMenu"), parser.isSet(QStringLiteral("preview-format-menu")));
    engine.rootContext()->setContextProperty(QStringLiteral("previewSlideshow"), parser.isSet(QStringLiteral("preview-slideshow")));
    engine.rootContext()->setContextProperty(QStringLiteral("previewTemplatePicker"), parser.isSet(QStringLiteral("preview-template-picker")));
    engine.load(QUrl(QStringLiteral("qrc:/qml/Main.qml")));
    if (engine.rootObjects().isEmpty()) {
        std::fprintf(stderr, "Could not load Nexus Office's QML interface.\n");
        return 1;
    }
    if (parser.isSet(QStringLiteral("screenshot"))) {
        auto *window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
        if (!window) return 1;
        QTimer::singleShot(600, window, [window, &app, &parser] {
            QQuickWindow *capture = window;
            if (parser.isSet(QStringLiteral("preview-slideshow"))) {
                for (QWindow *candidate : QGuiApplication::allWindows()) {
                    if (candidate->isVisible() && candidate != window) {
                        if (auto *quick = qobject_cast<QQuickWindow *>(candidate)) capture = quick;
                    }
                }
            }
            if (!capture->grabWindow().save(parser.value(QStringLiteral("screenshot"))))
                qWarning("Could not save screenshot.");
            app.quit();
        });
    } else if (parser.isSet(QStringLiteral("smoke")))
        QTimer::singleShot(400, &app, &QCoreApplication::quit);
    return app.exec();
}
