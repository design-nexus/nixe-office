#include "IconProvider.h"

#include <QColor>
#include <QFile>
#include <QPainter>
#include <QRegularExpression>
#include <QSvgRenderer>

QImage IconProvider::requestImage(const QString &id, QSize *size, const QSize &requestedSize) {
    const QString name = id.section(QLatin1Char('~'), 0, 0);
    const QString colorValue = id.section(QLatin1Char('~'), 1, 1);
    if (!QRegularExpression(QStringLiteral("^[a-z0-9-]+$")).match(name).hasMatch()) return {};
    QColor color(QStringLiteral("#") + colorValue);
    if (!color.isValid()) color = QColor(QStringLiteral("#ffffff"));

    QFile file(QStringLiteral(":/assets/icons/") + name + QStringLiteral(".svg"));
    if (!file.open(QIODevice::ReadOnly)) return {};
    QByteArray svg = file.readAll();
    svg.replace("currentColor", color.name(QColor::HexRgb).toUtf8());
    QSvgRenderer renderer(svg);
    if (!renderer.isValid()) return {};

    const int width = qBound(16, requestedSize.width() > 0 ? requestedSize.width() : 48, 128);
    const int height = qBound(16, requestedSize.height() > 0 ? requestedSize.height() : 48, 128);
    QImage image(width, height, QImage::Format_ARGB32_Premultiplied);
    image.fill(Qt::transparent);
    QPainter painter(&image);
    painter.setRenderHint(QPainter::Antialiasing);
    renderer.render(&painter);
    if (size) *size = image.size();
    return image;
}
