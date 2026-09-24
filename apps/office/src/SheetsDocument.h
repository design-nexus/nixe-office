#pragma once

#include <QAbstractTableModel>
#include <QHash>
#include <QSet>
#include <QUrl>
#include <QVector>
#include <QVariantList>
#include <QTimer>
#include <QJsonObject>

class SheetsDocument : public QAbstractTableModel {
    Q_OBJECT
    Q_PROPERTY(QString path READ path NOTIFY stateChanged)
    Q_PROPERTY(QString displayName READ displayName NOTIFY stateChanged)
    Q_PROPERTY(bool dirty READ dirty NOTIFY stateChanged)
    Q_PROPERTY(bool hasDocument READ hasDocument NOTIFY stateChanged)
    Q_PROPERTY(QString errorString READ errorString NOTIFY errorChanged)
    Q_PROPERTY(int revision READ revision NOTIFY stateChanged)
    Q_PROPERTY(int visibleRowCount READ visibleRowCount NOTIFY stateChanged)
    Q_PROPERTY(QString chartRange READ chartRange NOTIFY stateChanged)
    Q_PROPERTY(QString chartType READ chartType NOTIFY stateChanged)
    Q_PROPERTY(QVariantList chartData READ chartData NOTIFY stateChanged)
    Q_PROPERTY(QString filterQuery READ filterQuery NOTIFY stateChanged)
    Q_PROPERTY(bool canUndo READ canUndo NOTIFY historyChanged)
    Q_PROPERTY(bool canRedo READ canRedo NOTIFY historyChanged)
    Q_PROPERTY(bool recoveryAvailable READ recoveryAvailable NOTIFY recoveryAvailableChanged)
    Q_PROPERTY(QStringList sheetNames READ sheetNames NOTIFY sheetsChanged)
    Q_PROPERTY(int activeSheetIndex READ activeSheetIndex NOTIFY sheetsChanged)

public:
    explicit SheetsDocument(QObject *parent = nullptr);
    int rowCount(const QModelIndex &parent = {}) const override;
    int columnCount(const QModelIndex &parent = {}) const override;
    QVariant data(const QModelIndex &index, int role = Qt::DisplayRole) const override;
    QHash<int, QByteArray> roleNames() const override;

    QString path() const { return m_path; }
    QString displayName() const;
    bool dirty() const { return m_dirty; }
    bool hasDocument() const { return m_hasDocument; }
    QString errorString() const { return m_error; }
    int revision() const { return m_revision; }
    int visibleRowCount() const { return m_viewRows.size(); }
    QString chartRange() const { return m_chartRange; }
    QString chartType() const { return m_chartType; }
    QVariantList chartData() const;
    QString filterQuery() const { return m_filterQuery; }
    bool canUndo() const { return !m_undo.isEmpty(); }
    bool canRedo() const { return !m_redo.isEmpty(); }
    bool recoveryAvailable() const { return !m_availableRecovery.isEmpty(); }
    QStringList sheetNames() const;
    int activeSheetIndex() const { return m_activeSheet; }

    Q_INVOKABLE void newDocument();
    Q_INVOKABLE bool createTemplate(const QString &templateId);
    Q_INVOKABLE bool open(const QUrl &url);
    Q_INVOKABLE bool openPath(const QString &path) { return open(QUrl::fromLocalFile(path)); }
    Q_INVOKABLE bool save();
    Q_INVOKABLE bool saveAs(const QUrl &url);
    Q_INVOKABLE bool importXlsx(const QUrl &url);
    Q_INVOKABLE bool importXlsxPath(const QString &path) { return importXlsx(QUrl::fromLocalFile(path)); }
    Q_INVOKABLE bool exportXlsx(const QUrl &url);
    Q_INVOKABLE void setCell(int row, int column, const QString &raw);
    Q_INVOKABLE QString rawAt(int row, int column) const;
    Q_INVOKABLE QString displayAt(int row, int column) const;
    Q_INVOKABLE QString cellName(int row, int column) const;
    Q_INVOKABLE QString columnName(int column) const;
    Q_INVOKABLE int sourceRow(int viewRow) const;
    Q_INVOKABLE int numberFormatAt(int row, int column) const;
    Q_INVOKABLE bool boldAt(int row, int column) const;
    Q_INVOKABLE QString fillColorAt(int row, int column) const;
    Q_INVOKABLE QString textColorAt(int row, int column) const;
    Q_INVOKABLE int alignmentAt(int row, int column) const;
    Q_INVOKABLE void setNumberFormat(int row, int column, int format);
    Q_INVOKABLE void toggleBold(int row, int column);
    Q_INVOKABLE void setRangeNumberFormat(int firstRow, int firstColumn, int lastRow, int lastColumn, int format);
    Q_INVOKABLE void toggleRangeBold(int firstRow, int firstColumn, int lastRow, int lastColumn);
    Q_INVOKABLE void setRangeFillColor(int firstRow, int firstColumn, int lastRow, int lastColumn, const QString &color);
    Q_INVOKABLE void setRangeTextColor(int firstRow, int firstColumn, int lastRow, int lastColumn, const QString &color);
    Q_INVOKABLE void setRangeAlignment(int firstRow, int firstColumn, int lastRow, int lastColumn, int alignment);
    Q_INVOKABLE int rowHeight(int row) const;
    Q_INVOKABLE int columnWidth(int column) const;
    Q_INVOKABLE void setRowHeight(int row, int height);
    Q_INVOKABLE void setColumnWidth(int column, int width);
    Q_INVOKABLE void sortBy(int column, bool ascending);
    Q_INVOKABLE void clearSort();
    Q_INVOKABLE void setFilter(int column, const QString &query);
    Q_INVOKABLE void setChart(const QString &range, const QString &type);
    Q_INVOKABLE bool exportPdf(const QUrl &url);
    Q_INVOKABLE bool exportCsv(const QUrl &url);
    Q_INVOKABLE void beginDimensionResize();
    Q_INVOKABLE void endDimensionResize();
    Q_INVOKABLE void undo();
    Q_INVOKABLE void redo();
    Q_INVOKABLE void writeRecoveryNow();
    Q_INVOKABLE bool restoreRecovery();
    Q_INVOKABLE void discardRecovery();
    Q_INVOKABLE void discardSessionRecovery();
    Q_INVOKABLE void addSheet();
    Q_INVOKABLE void duplicateSheet();
    Q_INVOKABLE void renameSheet(int index, const QString &name);
    Q_INVOKABLE void removeSheet(int index);
    Q_INVOKABLE void selectSheet(int index);
    Q_INVOKABLE void copyRange(int firstRow, int firstColumn, int lastRow, int lastColumn);
    Q_INVOKABLE void pasteRange(int row, int column);
    Q_INVOKABLE void pasteRangeToSelection(int firstRow, int firstColumn, int lastRow, int lastColumn);
    Q_INVOKABLE void clearRange(int firstRow, int firstColumn, int lastRow, int lastColumn);
    Q_INVOKABLE void fillRange(int firstRow, int firstColumn, int lastRow, int lastColumn,
                               int targetRow, int targetColumn);
    Q_INVOKABLE void moveRange(int firstRow, int firstColumn, int lastRow, int lastColumn,
                               int targetRow, int targetColumn);

signals:
    void stateChanged();
    void dimensionsChanged();
    void historyChanged();
    void recoveryAvailableChanged();
    void errorChanged();
    void sheetsChanged();

private:
    static constexpr int rows = 500;
    static constexpr int columns = 52;
    int key(int row, int column) const { return row * columns + column; }
    QString evaluate(int row, int column, QSet<int> &visiting) const;
    void setError(const QString &error);
    void refresh();
    void rebuildView();
    bool saveNative(const QString &path);
    bool saveCsv(const QString &path);
    bool openNative(const QString &path);
    bool openCsv(const QString &path);
    bool hasNativeFeatures() const;
    QJsonObject nativeObject() const;
    struct SheetState {
        QString name = QStringLiteral("Sheet 1");
        QHash<int, QString> cells;
        QHash<int, int> numberFormats, alignments, rowHeights, columnWidths;
        QHash<int, QString> fillColors, textColors;
        QSet<int> boldCells;
        QVector<int> viewRows;
        int sortColumn = -1, filterColumn = -1;
        bool sortAscending = true;
        QString filterQuery, chartRange;
        QString chartType = QStringLiteral("bar");
    };
    struct HistoryState { QVector<SheetState> sheets; int active = 0; };
    SheetState activeState() const;
    void loadActiveState(const SheetState &state);
    QVector<SheetState> allSheets() const;
    bool parseSheetObject(const QJsonObject &object, SheetState &sheet);
    QJsonObject sheetObject(const SheetState &sheet) const;
    HistoryState capture() const;
    void restore(const HistoryState &state);
    void recordEdit();
    void clearHistory();
    QString recoveryDirectory() const;
    void findRecovery();
    void removeSessionRecovery();

    QHash<int, QString> m_cells;
    QVector<SheetState> m_sheets{SheetState()};
    int m_activeSheet = 0;
    QHash<int, int> m_numberFormats;
    QHash<int, int> m_alignments;
    QHash<int, QString> m_fillColors, m_textColors;
    QSet<int> m_boldCells;
    QHash<int, int> m_rowHeights;
    QHash<int, int> m_columnWidths;
    QVector<int> m_viewRows;
    int m_sortColumn = -1;
    bool m_sortAscending = true;
    int m_filterColumn = -1;
    QString m_filterQuery;
    QString m_chartRange;
    QString m_chartType = QStringLiteral("bar");
    QString m_path;
    QString m_error;
    bool m_dirty = false;
    bool m_hasDocument = false;
    int m_revision = 0;
    QVector<HistoryState> m_undo, m_redo;
    QByteArray m_savedState;
    QTimer m_recoveryTimer;
    QString m_sessionRecovery, m_availableRecovery;
    bool m_dimensionResizeActive = false;
    bool m_dimensionResizeRecorded = false;
};
