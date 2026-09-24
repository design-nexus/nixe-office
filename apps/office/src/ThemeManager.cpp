#include "ThemeManager.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QRegularExpression>
#include <QTimer>

namespace {
QString validColor(const QVariantMap &values, const QString &key, const QString &fallback) {
    const QString value = values.value(key).toString();
    return QColor(value).isValid() ? value : fallback;
}

QString textOnAccent(const QString &accent) {
    const QColor color(accent);
    const double luminance = 0.2126 * color.redF() + 0.7152 * color.greenF() + 0.0722 * color.blueF();
    return luminance > 0.60 ? QStringLiteral("#171923") : QStringLiteral("#ffffff");
}
}

ThemeManager::ThemeManager(QObject *parent)
    : QObject(parent),
      m_themePath(QDir::homePath() + QStringLiteral("/.local/state/omarchy/current/theme/colors.toml")),
      m_settings(QStringLiteral("Nexus"), QStringLiteral("OfficeSuite")) {
    loadBuiltIns();
    m_selectedTheme = m_settings.value(QStringLiteral("appearance/theme"), QStringLiteral("auto")).toString();
    connect(&m_watcher, &QFileSystemWatcher::fileChanged, this, [this] {
        QTimer::singleShot(100, this, [this] { refresh(); });
    });
    connect(&m_watcher, &QFileSystemWatcher::directoryChanged, this, [this] {
        QTimer::singleShot(100, this, [this] { refresh(); });
    });
    refresh();
}

void ThemeManager::setSelectedTheme(const QString &theme) {
    if (theme != QStringLiteral("auto") && builtInPalette(theme).isEmpty())
        return;
    if (theme == m_selectedTheme)
        return;
    m_selectedTheme = theme;
    m_settings.setValue(QStringLiteral("appearance/theme"), theme);
    emit selectedThemeChanged();
    refresh();
}

QVariantMap ThemeManager::parseOmarchyPalette(const QByteArray &contents) {
    QVariantMap source;
    const QRegularExpression entry(QStringLiteral(R"re(^\s*([a-z_]+)\s*=\s*"([^"]+)"\s*(?:#.*)?$)re"));
    for (const QByteArray &line : contents.split('\n')) {
        const auto match = entry.match(QString::fromUtf8(line));
        if (match.hasMatch())
            source.insert(match.captured(1), match.captured(2));
    }
    if (!QColor(source.value(QStringLiteral("background")).toString()).isValid() ||
        !QColor(source.value(QStringLiteral("foreground")).toString()).isValid())
        return {};

    QVariantMap result;
    result.insert(QStringLiteral("dark"), source.value(QStringLiteral("mode")) != QStringLiteral("light"));
    result.insert(QStringLiteral("background"), validColor(source, QStringLiteral("background"), QStringLiteral("#1e1e2e")));
    result.insert(QStringLiteral("surface"), validColor(source, QStringLiteral("dark_background"), result.value(QStringLiteral("background")).toString()));
    result.insert(QStringLiteral("raised"), validColor(source, QStringLiteral("lighter_background"), result.value(QStringLiteral("surface")).toString()));
    result.insert(QStringLiteral("border"), validColor(source, QStringLiteral("selection"), result.value(QStringLiteral("raised")).toString()));
    result.insert(QStringLiteral("text"), validColor(source, QStringLiteral("foreground"), QStringLiteral("#cdd6f4")));
    result.insert(QStringLiteral("muted"), validColor(source, QStringLiteral("dark_foreground"), result.value(QStringLiteral("text")).toString()));
    result.insert(QStringLiteral("accent"), validColor(source, QStringLiteral("accent"), QStringLiteral("#89b4fa")));
    result.insert(QStringLiteral("danger"), validColor(source, QStringLiteral("red"), QStringLiteral("#f38ba8")));
    result.insert(QStringLiteral("accentText"), textOnAccent(result.value(QStringLiteral("accent")).toString()));
    return result;
}

void ThemeManager::loadBuiltIns() {
    QFile file(QStringLiteral(":/assets/themes.json"));
    if (!file.open(QIODevice::ReadOnly))
        return;
    const QJsonDocument document = QJsonDocument::fromJson(file.readAll());
    for (const QJsonValue &value : document.array()) {
        const QJsonObject object = value.toObject();
        if (object.contains(QStringLiteral("id")) && object.contains(QStringLiteral("name")))
            m_themes.append(object.toVariantMap());
    }
}

QVariantMap ThemeManager::builtInPalette(const QString &id) const {
    for (const QVariant &entry : m_themes) {
        const QVariantMap theme = entry.toMap();
        if (theme.value(QStringLiteral("id")) == id) {
            QVariantMap palette = theme.value(QStringLiteral("palette")).toMap();
            palette.insert(QStringLiteral("accentText"), textOnAccent(palette.value(QStringLiteral("accent")).toString()));
            return palette;
        }
    }
    return {};
}

void ThemeManager::watchOmarchyPaths() {
    const QStringList existing = m_watcher.files() + m_watcher.directories();
    if (!existing.isEmpty())
        m_watcher.removePaths(existing);
    const QString statePath = QDir::homePath() + QStringLiteral("/.local/state/omarchy");
    for (const QString &path : {statePath, statePath + QStringLiteral("/current"),
                                statePath + QStringLiteral("/current/theme"), m_themePath}) {
        if (QFileInfo::exists(path) && !m_watcher.files().contains(path) && !m_watcher.directories().contains(path))
            m_watcher.addPath(path);
    }
}

void ThemeManager::refresh() {
    watchOmarchyPaths();
    const bool available = QFileInfo::exists(m_themePath);
    const bool availabilityChanged = available != m_omarchyAvailable;
    m_omarchyAvailable = available;
    QVariantMap next;
    QString name;
    if (m_selectedTheme == QStringLiteral("auto") && available) {
        QFile file(m_themePath);
        if (file.open(QIODevice::ReadOnly))
            next = parseOmarchyPalette(file.readAll());
        if (!next.isEmpty())
            name = QStringLiteral("Omarchy system");
    }
    if (next.isEmpty()) {
        const QString fallback = m_selectedTheme == QStringLiteral("auto") ? QStringLiteral("catppuccin") : m_selectedTheme;
        next = builtInPalette(fallback);
        for (const QVariant &entry : m_themes) {
            const QVariantMap item = entry.toMap();
            if (item.value(QStringLiteral("id")) == fallback)
                name = item.value(QStringLiteral("name")).toString();
        }
    }
    if (next != m_palette || name != m_activeThemeName || availabilityChanged) {
        m_palette = next;
        m_activeThemeName = name;
        emit paletteChanged();
    }
}
