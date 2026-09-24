#pragma once

#include <QColor>
#include <QFileSystemWatcher>
#include <QObject>
#include <QSettings>
#include <QVariantList>
#include <QVariantMap>

class ThemeManager : public QObject {
    Q_OBJECT
    Q_PROPERTY(QString selectedTheme READ selectedTheme WRITE setSelectedTheme NOTIFY selectedThemeChanged)
    Q_PROPERTY(QString activeThemeName READ activeThemeName NOTIFY paletteChanged)
    Q_PROPERTY(QVariantMap palette READ palette NOTIFY paletteChanged)
    Q_PROPERTY(QVariantList themes READ themes CONSTANT)
    Q_PROPERTY(bool omarchyAvailable READ omarchyAvailable NOTIFY paletteChanged)

public:
    explicit ThemeManager(QObject *parent = nullptr);

    QString selectedTheme() const { return m_selectedTheme; }
    void setSelectedTheme(const QString &theme);
    QString activeThemeName() const { return m_activeThemeName; }
    QVariantMap palette() const { return m_palette; }
    QVariantList themes() const { return m_themes; }
    bool omarchyAvailable() const { return m_omarchyAvailable; }

    static QVariantMap parseOmarchyPalette(const QByteArray &contents);

signals:
    void selectedThemeChanged();
    void paletteChanged();

private:
    void loadBuiltIns();
    void refresh();
    void watchOmarchyPaths();
    QVariantMap builtInPalette(const QString &id) const;
    QString m_themePath;
    QString m_selectedTheme;
    QString m_activeThemeName;
    QVariantMap m_palette;
    QVariantList m_themes;
    bool m_omarchyAvailable = false;
    QSettings m_settings;
    QFileSystemWatcher m_watcher;
};
