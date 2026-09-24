#include "AppController.h"

#include <QCoreApplication>
#include <QFileInfo>
#include <QProcess>

AppController::AppController(const QString &appId, QObject *parent)
    : QObject(parent), m_appId(appId), m_settings(QStringLiteral("Nexus"), QStringLiteral("OfficeSuite")) {
    m_recentFiles = m_settings.value(settingsKey(QStringLiteral("recentFiles"))).toStringList();
    m_windowWidth = m_settings.value(settingsKey(QStringLiteral("windowWidth")), 1240).toInt();
    m_windowHeight = m_settings.value(settingsKey(QStringLiteral("windowHeight")), 800).toInt();
}

QString AppController::appTitle() const {
    if (m_appId == QStringLiteral("sheets")) return QStringLiteral("Nexus Sheets");
    if (m_appId == QStringLiteral("present")) return QStringLiteral("Nexus Present");
    return QStringLiteral("Nexus Write");
}

QString AppController::appDescription() const {
    if (m_appId == QStringLiteral("sheets")) return QStringLiteral("For budgets, plans, and everyday numbers.");
    if (m_appId == QStringLiteral("present")) return QStringLiteral("Make a story worth sharing.");
    return QStringLiteral("Make something worth reading.");
}

QString AppController::settingsKey(const QString &suffix) const {
    return QStringLiteral("apps/") + m_appId + QLatin1Char('/') + suffix;
}

void AppController::recordRecentFile(const QString &path) {
    const QString absolute = QFileInfo(path).absoluteFilePath();
    if (path.isEmpty() || absolute.isEmpty()) return;
    m_recentFiles.removeAll(absolute);
    m_recentFiles.prepend(absolute);
    while (m_recentFiles.size() > 10) m_recentFiles.removeLast();
    m_settings.setValue(settingsKey(QStringLiteral("recentFiles")), m_recentFiles);
    emit recentFilesChanged();
}

void AppController::clearRecentFiles() {
    if (m_recentFiles.isEmpty()) return;
    m_recentFiles.clear();
    m_settings.setValue(settingsKey(QStringLiteral("recentFiles")), m_recentFiles);
    emit recentFilesChanged();
}

void AppController::setWindowSize(int width, int height) {
    if (width < 760 || height < 520 || (width == m_windowWidth && height == m_windowHeight)) return;
    m_windowWidth = width;
    m_windowHeight = height;
    m_settings.setValue(settingsKey(QStringLiteral("windowWidth")), width);
    m_settings.setValue(settingsKey(QStringLiteral("windowHeight")), height);
    emit windowSizeChanged();
}

void AppController::launchApp(const QString &appId) {
    if (appId != QStringLiteral("write") && appId != QStringLiteral("sheets") && appId != QStringLiteral("present"))
        return;
    if (appId == m_appId) return;
    QProcess::startDetached(QCoreApplication::applicationFilePath(), {QStringLiteral("--app"), appId});
}
