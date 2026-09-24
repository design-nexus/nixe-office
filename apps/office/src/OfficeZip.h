#pragma once

#include <QDir>
#include <QFile>
#include <QHash>
#include <QMap>
#include <QSaveFile>
#include <QTemporaryFile>
#include <QXmlStreamReader>
#include <zip.h>

namespace OfficeZip {
inline QByteArray read(zip_t *archive, const QString &name, zip_uint64_t maximum = 20 * 1024 * 1024) {
    const QByteArray utf8 = name.toUtf8();
    zip_stat_t stat;
    zip_stat_init(&stat);
    if (zip_stat(archive, utf8.constData(), 0, &stat) != 0 || stat.size > maximum) return {};
    zip_file_t *file = zip_fopen(archive, utf8.constData(), 0);
    if (!file) return {};
    QByteArray data(static_cast<qsizetype>(stat.size), '\0');
    const zip_int64_t count = zip_fread(file, data.data(), stat.size);
    zip_fclose(file);
    return count == static_cast<zip_int64_t>(stat.size) ? data : QByteArray();
}
inline QHash<QString, QString> relationships(const QByteArray &xml) {
    QHash<QString, QString> result;
    QXmlStreamReader reader(xml);
    while (!reader.atEnd()) {
        reader.readNext();
        if (reader.isStartElement() && reader.name() == QLatin1StringView("Relationship") &&
            reader.attributes().value(QStringLiteral("TargetMode")) != QLatin1StringView("External")) {
            const QString id = reader.attributes().value(QStringLiteral("Id")).toString();
            const QString target = reader.attributes().value(QStringLiteral("Target")).toString();
            if (!id.isEmpty() && !target.isEmpty()) result.insert(id, target);
        }
    }
    return reader.hasError() ? QHash<QString, QString>() : result;
}
inline QString resolve(const QString &folder, const QString &target) {
    if (target.isEmpty() || target.contains(QLatin1Char('\\')) || target.contains(QLatin1Char(':'))) return {};
    const QString path = QDir::cleanPath(target.startsWith(QLatin1Char('/')) ? target.mid(1) :
                                           folder.isEmpty() ? target : folder + QLatin1Char('/') + target);
    return path == QStringLiteral("..") || path.startsWith(QStringLiteral("../")) || path.startsWith(QLatin1Char('/')) ? QString() : path;
}
inline bool writePackage(const QString &path, const QMap<QString, QByteArray> &parts) {
    QTemporaryFile temporary(QDir::tempPath() + QStringLiteral("/nexus-office-XXXXXX"));
    if (!temporary.open()) return false;
    const QString tempPath = temporary.fileName();
    temporary.close();
    int error = 0;
    zip_t *archive = zip_open(QFile::encodeName(tempPath).constData(), ZIP_CREATE | ZIP_TRUNCATE, &error);
    if (!archive) return false;
    for (auto it = parts.cbegin(); it != parts.cend(); ++it) {
        const QByteArray name = it.key().toUtf8();
        zip_source_t *source = zip_source_buffer(archive, it.value().constData(), it.value().size(), 0);
        if (!source || zip_file_add(archive, name.constData(), source, ZIP_FL_ENC_UTF_8) < 0) {
            if (source) zip_source_free(source);
            zip_discard(archive);
            return false;
        }
    }
    if (zip_close(archive) != 0) { zip_discard(archive); return false; }
    QFile source(tempPath);
    if (!source.open(QIODevice::ReadOnly)) return false;
    const QByteArray data = source.readAll();
    QSaveFile destination(path);
    return destination.open(QIODevice::WriteOnly) && destination.write(data) == data.size() && destination.commit();
}
}
