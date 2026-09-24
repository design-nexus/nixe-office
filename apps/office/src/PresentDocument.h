#pragma once

#include <QAbstractListModel>
#include <QColor>
#include <QUrl>
#include <QVariantMap>
#include <QVector>
#include <QTimer>
#include <QJsonObject>

class PresentDocument : public QAbstractListModel {
    Q_OBJECT
    Q_PROPERTY(QString path READ path NOTIFY stateChanged)
    Q_PROPERTY(QString displayName READ displayName NOTIFY stateChanged)
    Q_PROPERTY(bool dirty READ dirty NOTIFY stateChanged)
    Q_PROPERTY(bool hasDocument READ hasDocument NOTIFY stateChanged)
    Q_PROPERTY(QString errorString READ errorString NOTIFY errorChanged)
    Q_PROPERTY(int slideCount READ slideCount NOTIFY stateChanged)
    Q_PROPERTY(int selectedIndex READ selectedIndex NOTIFY selectionChanged)
    Q_PROPERTY(QString currentTitle READ currentTitle NOTIFY currentSlideChanged)
    Q_PROPERTY(QString currentBody READ currentBody NOTIFY currentSlideChanged)
    Q_PROPERTY(QString currentNotes READ currentNotes NOTIFY currentSlideChanged)
    Q_PROPERTY(int currentLayout READ currentLayout NOTIFY currentSlideChanged)
    Q_PROPERTY(int currentStyle READ currentStyle NOTIFY currentSlideChanged)
    Q_PROPERTY(QString currentFontFamily READ currentFontFamily NOTIFY currentSlideChanged)
    Q_PROPERTY(QString currentFontColor READ currentFontColor NOTIFY currentSlideChanged)
    Q_PROPERTY(QString currentBackgroundColor READ currentBackgroundColor NOTIFY currentSlideChanged)
    Q_PROPERTY(bool currentBold READ currentBold NOTIFY currentSlideChanged)
    Q_PROPERTY(bool currentItalic READ currentItalic NOTIFY currentSlideChanged)
    Q_PROPERTY(int currentTitleSize READ currentTitleSize NOTIFY currentSlideChanged)
    Q_PROPERTY(int currentBodySize READ currentBodySize NOTIFY currentSlideChanged)
    Q_PROPERTY(QVariantList currentElements READ currentElements NOTIFY currentSlideChanged)
    Q_PROPERTY(bool canUndo READ canUndo NOTIFY historyChanged)
    Q_PROPERTY(bool canRedo READ canRedo NOTIFY historyChanged)
    Q_PROPERTY(bool recoveryAvailable READ recoveryAvailable NOTIFY recoveryAvailableChanged)

public:
    explicit PresentDocument(QObject *parent = nullptr);
    int rowCount(const QModelIndex &parent = {}) const override;
    QVariant data(const QModelIndex &index, int role = Qt::DisplayRole) const override;
    QHash<int, QByteArray> roleNames() const override;

    QString path() const { return m_path; }
    QString displayName() const;
    bool dirty() const { return m_dirty; }
    bool hasDocument() const { return m_hasDocument; }
    QString errorString() const { return m_error; }
    int slideCount() const { return m_slides.size(); }
    int selectedIndex() const { return m_selected; }
    QString currentTitle() const;
    QString currentBody() const;
    QString currentNotes() const;
    int currentLayout() const;
    int currentStyle() const;
    QString currentFontFamily() const;
    QString currentFontColor() const;
    QString currentBackgroundColor() const;
    bool currentBold() const;
    bool currentItalic() const;
    int currentTitleSize() const;
    int currentBodySize() const;
    QVariantList currentElements() const;
    bool canUndo() const { return !m_undo.isEmpty(); }
    bool canRedo() const { return !m_redo.isEmpty(); }
    bool recoveryAvailable() const { return !m_availableRecovery.isEmpty(); }

    Q_INVOKABLE void newDocument();
    Q_INVOKABLE bool createTemplate(const QString &templateId);
    Q_INVOKABLE bool open(const QUrl &url);
    Q_INVOKABLE bool openPath(const QString &path) { return open(QUrl::fromLocalFile(path)); }
    Q_INVOKABLE bool save();
    Q_INVOKABLE bool saveAs(const QUrl &url);
    Q_INVOKABLE bool importPptx(const QUrl &url);
    Q_INVOKABLE bool importPptxPath(const QString &path) { return importPptx(QUrl::fromLocalFile(path)); }
    Q_INVOKABLE bool exportPptx(const QUrl &url);
    Q_INVOKABLE bool exportPdf(const QUrl &url, const QColor &accent);
    Q_INVOKABLE QVariantMap slideAt(int index) const;
    Q_INVOKABLE void selectSlide(int index);
    Q_INVOKABLE void addSlide();
    Q_INVOKABLE void duplicateSlide();
    Q_INVOKABLE void deleteSlide();
    Q_INVOKABLE void moveSlide(int direction);
    Q_INVOKABLE void setTitle(const QString &title);
    Q_INVOKABLE void setBody(const QString &body);
    Q_INVOKABLE void setNotes(const QString &notes);
    Q_INVOKABLE void setLayout(int layout);
    Q_INVOKABLE void setStyle(int style);
    Q_INVOKABLE void setFontFamily(const QString &family);
    Q_INVOKABLE void setFontColor(const QString &color);
    Q_INVOKABLE void setBackgroundColor(const QString &color);
    Q_INVOKABLE void setBold(bool bold);
    Q_INVOKABLE void setItalic(bool italic);
    Q_INVOKABLE void setTitleSize(int size);
    Q_INVOKABLE void setBodySize(int size);
    Q_INVOKABLE bool addImage(const QUrl &url);
    Q_INVOKABLE void addTextBox();
    Q_INVOKABLE void addShape(const QString &shape);
    Q_INVOKABLE void setElementFill(const QString &id, const QString &color);
    Q_INVOKABLE void alignElement(const QString &id, const QString &alignment);
    Q_INVOKABLE void setElementText(const QString &id, const QString &text);
    Q_INVOKABLE void setElementRect(const QString &id, int x, int y, int width, int height);
    Q_INVOKABLE void removeElement(const QString &id);
    Q_INVOKABLE void undo();
    Q_INVOKABLE void redo();
    Q_INVOKABLE void writeRecoveryNow();
    Q_INVOKABLE bool restoreRecovery();
    Q_INVOKABLE void discardRecovery();
    Q_INVOKABLE void discardSessionRecovery();

signals:
    void stateChanged();
    void errorChanged();
    void selectionChanged();
    void currentSlideChanged();
    void historyChanged();
    void recoveryAvailableChanged();

private:
    struct Element {
        QString id;
        QString type;
        int x=120,y=120,width=320,height=180;
        QString text;
        QString shape;
        QString fill;
        QByteArray image;
    };
    struct Slide {
        QString title;
        QString body;
        QString notes;
        int layout = 0;
        int style = 0;
        QString fontFamily = QStringLiteral("Noto Sans");
        QString fontColor;
        QString backgroundColor;
        bool bold = false;
        bool italic = false;
        int titleSize = 0;
        int bodySize = 0;
        QVector<Element> elements;
    };
    Slide *current();
    const Slide *current() const;
    QVariantList elementsFor(const Slide &slide) const;
    void changed();
    void setError(const QString &error);
    struct HistoryState { QVector<Slide> slides; int selected = 0; };
    HistoryState capture() const;
    void restore(const HistoryState &state);
    void recordEdit();
    void recordTextEdit(const QString &field);
    void clearHistory();
    QJsonObject nativeObject() const;
    QString recoveryDirectory() const;
    void findRecovery();
    void removeSessionRecovery();

    QVector<Slide> m_slides;
    int m_selected = -1;
    QString m_path;
    QString m_error;
    bool m_dirty = false;
    bool m_hasDocument = false;
    QVector<HistoryState> m_undo, m_redo;
    QByteArray m_savedState;
    QTimer m_recoveryTimer;
    QString m_sessionRecovery, m_availableRecovery;
    QString m_lastTextEditKey;
    qint64 m_lastTextEditAt = 0;
};
