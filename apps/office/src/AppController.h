#pragma once

#include <QObject>
#include <QSettings>
#include <QStringList>
#include <QUrl>

class AppController : public QObject {
    Q_OBJECT
    Q_PROPERTY(QString appId READ appId CONSTANT)
    Q_PROPERTY(QString appTitle READ appTitle CONSTANT)
    Q_PROPERTY(QString appDescription READ appDescription CONSTANT)
    Q_PROPERTY(QStringList recentFiles READ recentFiles NOTIFY recentFilesChanged)
    Q_PROPERTY(int windowWidth READ windowWidth NOTIFY windowSizeChanged)
    Q_PROPERTY(int windowHeight READ windowHeight NOTIFY windowSizeChanged)

public:
    explicit AppController(const QString &appId, QObject *parent = nullptr);

    QString appId() const { return m_appId; }
    QString appTitle() const;
    QString appDescription() const;
    QStringList recentFiles() const { return m_recentFiles; }
    int windowWidth() const { return m_windowWidth; }
    int windowHeight() const { return m_windowHeight; }

    Q_INVOKABLE void recordRecentFile(const QString &path);
    Q_INVOKABLE void recordRecentUrl(const QUrl &url) { if (url.isLocalFile()) recordRecentFile(url.toLocalFile()); }
    Q_INVOKABLE void clearRecentFiles();
    Q_INVOKABLE void setWindowSize(int width, int height);
    Q_INVOKABLE void launchApp(const QString &appId);

signals:
    void recentFilesChanged();
    void windowSizeChanged();

private:
    QString settingsKey(const QString &suffix) const;
    QString m_appId;
    QStringList m_recentFiles;
    int m_windowWidth;
    int m_windowHeight;
    QSettings m_settings;
};
