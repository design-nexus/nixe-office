#pragma once

#include <QQuickImageProvider>

class IconProvider : public QQuickImageProvider {
public:
    IconProvider() : QQuickImageProvider(QQuickImageProvider::Image) { }
    QImage requestImage(const QString &id, QSize *size, const QSize &requestedSize) override;
};
