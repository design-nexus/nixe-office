#include "IconProvider.h"

#include <QColor>
#include <QtTest>

class IconTests : public QObject {
    Q_OBJECT
private slots:
    void rendersTintedIcon();
    void rejectsUnknownIcon();
};

void IconTests::rendersTintedIcon() {
    IconProvider icons;
    QSize size;
    const QImage image = icons.requestImage(QStringLiteral("file-text~89b4fa"), &size, QSize(48, 48));
    QCOMPARE(size, QSize(48, 48));
    QVERIFY(!image.isNull());
    bool found = false;
    for (int y = 0; y < image.height() && !found; ++y) {
        for (int x = 0; x < image.width(); ++x) {
            const QColor pixel = image.pixelColor(x, y);
            if (pixel.alpha() > 250) {
                QCOMPARE(pixel.red(), 0x89);
                QCOMPARE(pixel.green(), 0xb4);
                QCOMPARE(pixel.blue(), 0xfa);
                found = true;
                break;
            }
        }
    }
    QVERIFY(found);
}

void IconTests::rejectsUnknownIcon() {
    IconProvider icons;
    QVERIFY(icons.requestImage(QStringLiteral("../secret~ffffff"), nullptr, QSize()).isNull());
    QVERIFY(icons.requestImage(QStringLiteral("missing~ffffff"), nullptr, QSize()).isNull());
}

QTEST_GUILESS_MAIN(IconTests)
#include "IconTests.moc"
