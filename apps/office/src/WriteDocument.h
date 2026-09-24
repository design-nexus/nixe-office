#pragma once

#include <QTextDocument>
#include <QTextCursor>
#include <QObject>
#include <QTimer>
#include <QUrl>
#include <QMap>
#include <QByteArray>
#include <QVariantMap>
#include <QJsonObject>

struct _EnchantBroker;
struct _EnchantDict;

class QQuickTextDocument;

class WriteDocument : public QObject {
    Q_OBJECT
    Q_PROPERTY(QString path READ path NOTIFY stateChanged)
    Q_PROPERTY(QString displayName READ displayName NOTIFY stateChanged)
    Q_PROPERTY(bool dirty READ dirty NOTIFY stateChanged)
    Q_PROPERTY(int pageCount READ pageCount NOTIFY stateChanged)
    Q_PROPERTY(QString errorString READ errorString NOTIFY errorChanged)
    Q_PROPERTY(bool recoveryAvailable READ recoveryAvailable NOTIFY recoveryAvailableChanged)
    Q_PROPERTY(bool hasDocument READ hasDocument NOTIFY stateChanged)
    Q_PROPERTY(bool spellcheckAvailable READ spellcheckAvailable CONSTANT)
    Q_PROPERTY(int topMargin READ topMargin NOTIFY stateChanged)
    Q_PROPERTY(int bottomMargin READ bottomMargin NOTIFY stateChanged)
    Q_PROPERTY(int leftMargin READ leftMargin NOTIFY stateChanged)
    Q_PROPERTY(int rightMargin READ rightMargin NOTIFY stateChanged)
    Q_PROPERTY(QString headerText READ headerText NOTIFY stateChanged)
    Q_PROPERTY(QString footerText READ footerText NOTIFY stateChanged)
    Q_PROPERTY(bool showPageNumbers READ showPageNumbers NOTIFY stateChanged)

public:
    explicit WriteDocument(QObject *parent = nullptr);
    ~WriteDocument() override;
    QTextDocument *document() { return &m_document; }
    QString path() const { return m_path; }
    QString displayName() const;
    bool dirty() const { return m_document.isModified() || m_pageDirty; }
    int pageCount() const { return qMax(1, m_document.pageCount()); }
    QString errorString() const { return m_error; }
    bool recoveryAvailable() const { return !m_availableRecovery.isEmpty(); }
    bool hasDocument() const { return m_hasDocument; }
    int topMargin() const { return m_topMargin; }
    int bottomMargin() const { return m_bottomMargin; }
    int leftMargin() const { return m_leftMargin; }
    int rightMargin() const { return m_rightMargin; }
    QString headerText() const { return m_headerText; }
    QString footerText() const { return m_footerText; }
    bool showPageNumbers() const { return m_showPageNumbers; }
    bool spellcheckAvailable() const {
#ifdef NEXUS_HAVE_ENCHANT
        return true;
#else
        return false;
#endif
    }

    Q_INVOKABLE void attachEditor(QObject *textDocument);
    Q_INVOKABLE void newDocument();
    Q_INVOKABLE bool createTemplate(const QString &templateId);
    Q_INVOKABLE bool open(const QUrl &url);
    Q_INVOKABLE bool openPath(const QString &path) { return open(QUrl::fromLocalFile(path)); }
    Q_INVOKABLE bool save();
    Q_INVOKABLE bool saveAs(const QUrl &url);
    Q_INVOKABLE bool importDocx(const QUrl &url);
    Q_INVOKABLE bool importDocxPath(const QString &path) { return importDocx(QUrl::fromLocalFile(path)); }
    Q_INVOKABLE bool exportDocx(const QUrl &url);
    Q_INVOKABLE void toggleBold(int start, int end);
    Q_INVOKABLE void toggleItalic(int start, int end);
    Q_INVOKABLE void setFontFamily(int start, int end, const QString &family);
    Q_INVOKABLE void setFontSize(int start, int end, int points);
    Q_INVOKABLE void setFontColor(int start, int end, const QColor &color);
    Q_INVOKABLE void setHeading(int position, int level);
    Q_INVOKABLE void toggleList(int position, bool ordered);
    Q_INVOKABLE bool insertImage(int position, const QUrl &url);
    Q_INVOKABLE QVariantMap imageAt(int position) const;
    Q_INVOKABLE bool setImageWidth(int position, int width);
    Q_INVOKABLE bool setImageAlignment(int position, const QString &alignment);
    Q_INVOKABLE void insertTable(int position, int rows, int columns);
    Q_INVOKABLE bool exportPdf(const QUrl &url);
    Q_INVOKABLE void setPageLayout(int top, int bottom, int left, int right,
                                   const QString &header, const QString &footer, bool pageNumbers);
    Q_INVOKABLE QVariantMap spellingAt(int position);
    Q_INVOKABLE void replaceWordAt(int position, const QString &replacement);
    Q_INVOKABLE void undo();
    Q_INVOKABLE void redo();
    Q_INVOKABLE void writeRecoveryNow();
    Q_INVOKABLE bool restoreRecovery();
    Q_INVOKABLE void discardRecovery();
    Q_INVOKABLE void discardSessionRecovery();

signals:
    void stateChanged();
    void errorChanged();
    void recoveryAvailableChanged();
    void saved(const QString &path);

private:
    void setError(const QString &message);
    void removeSessionRecovery();
    QString recoveryDirectory() const;
    void findRecovery();
    void applyPageLayout();
    QJsonObject pageSettings() const;
    bool loadPageSettings(const QJsonObject &object);
    void reflowAlignedImages();
    QTextCursor cursorForRange(int start, int end);
    QTextDocument m_document;
    QString m_path;
    QString m_error;
    QString m_sessionRecovery;
    QString m_availableRecovery;
    bool m_hasDocument = false;
    QMap<QString, QByteArray> m_images;
    _EnchantBroker *m_spellBroker = nullptr;
    _EnchantDict *m_spellDict = nullptr;
    QTimer m_recoveryTimer;
    int m_topMargin = 54, m_bottomMargin = 54, m_leftMargin = 54, m_rightMargin = 54;
    QString m_headerText, m_footerText;
    bool m_showPageNumbers = false;
    bool m_pageDirty = false;
};
