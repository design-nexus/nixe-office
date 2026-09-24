#include "SheetsDocument.h"

#include <QFile>
#include <QFileInfo>
#include <QRegularExpression>
#include <QSaveFile>
#include <QJsonDocument>
#include <QUuid>
#include <functional>
#include <cmath>
#include <algorithm>

namespace {
struct Value {
    double number = 0;
    QString error;
    bool numeric = true;
    bool populated = true;
};

class FormulaParser {
public:
    FormulaParser(QString expression, std::function<Value(int, int)> cell)
        : m_expression(std::move(expression)), m_cell(std::move(cell)) {}

    Value parse() {
        Value value = expression();
        spaces();
        if (m_position != m_expression.size() && value.error.isEmpty()) value.error = QStringLiteral("#ERROR!");
        return value;
    }

private:
    void spaces() {
        while (m_position < m_expression.size() && m_expression.at(m_position).isSpace()) ++m_position;
    }
    bool take(QChar character) {
        spaces();
        if (m_position < m_expression.size() && m_expression.at(m_position) == character) {
            ++m_position;
            return true;
        }
        return false;
    }
    bool reference(int &row, int &column) {
        spaces();
        const int start = m_position;
        int col = 0;
        while (m_position < m_expression.size() && m_expression.at(m_position).isLetter()) {
            const ushort letter = m_expression.at(m_position).toUpper().unicode();
            if (letter < 'A' || letter > 'Z') break;
            col = qMin(10000, col * 26 + letter - 'A' + 1);
            ++m_position;
        }
        if (m_position == start || m_position >= m_expression.size() || !m_expression.at(m_position).isDigit()) {
            m_position = start;
            return false;
        }
        const int digitStart = m_position;
        while (m_position < m_expression.size() && m_expression.at(m_position).isDigit()) ++m_position;
        bool ok = false;
        const int parsedRow = m_expression.mid(digitStart, m_position - digitStart).toInt(&ok);
        row = ok ? parsedRow - 1 : -1;
        column = col - 1;
        return true;
    }
    Value expression() {
        Value left = term();
        while (true) {
            spaces();
            if (take('+')) {
                Value right = term();
                if (left.error.isEmpty()) left.error = right.error;
                left.number += right.number;
                left.numeric = left.populated = true;
            } else if (take('-')) {
                Value right = term();
                if (left.error.isEmpty()) left.error = right.error;
                left.number -= right.number;
                left.numeric = left.populated = true;
            } else break;
        }
        return left;
    }
    Value term() {
        Value left = factor();
        while (true) {
            spaces();
            if (take('*')) {
                Value right = factor();
                if (left.error.isEmpty()) left.error = right.error;
                left.number *= right.number;
                left.numeric = left.populated = true;
            } else if (take('/')) {
                Value right = factor();
                if (left.error.isEmpty()) left.error = right.error;
                if (right.number == 0 && left.error.isEmpty()) left.error = QStringLiteral("#DIV/0!");
                else if (right.number != 0) left.number /= right.number;
                left.numeric = left.populated = true;
            } else break;
        }
        return left;
    }
    Value function(const QString &name) {
        QList<Value> values;
        if (!take(')')) {
            while (true) {
                const int start = m_position;
                int firstRow = -1, firstColumn = -1;
                if (reference(firstRow, firstColumn) && take(':')) {
                    int lastRow = -1, lastColumn = -1;
                    if (!reference(lastRow, lastColumn) || firstRow < 0 || firstColumn < 0 ||
                        lastRow < firstRow || lastColumn < firstColumn || lastRow - firstRow > 499 ||
                        lastColumn - firstColumn > 51)
                        return {0, QStringLiteral("#REF!")};
                    for (int row = firstRow; row <= lastRow; ++row)
                        for (int column = firstColumn; column <= lastColumn; ++column)
                            values.append(m_cell(row, column));
                } else {
                    m_position = start;
                    values.append(expression());
                }
                if (take(')')) break;
                if (!take(',')) return {0, QStringLiteral("#ERROR!")};
            }
        }
        for (const Value &value : values)
            if (!value.error.isEmpty()) return value;
        QList<double> numbers;
        for(const Value &value:values)if(value.numeric)numbers.append(value.number);
        if(name==QStringLiteral("COUNT"))return {double(numbers.size()),{}};
        if(name==QStringLiteral("COUNTA")){
            int count=0;for(const Value &value:values)if(value.populated)++count;
            return {double(count),{}};
        }
        if (name == QStringLiteral("SUM")) {
            double total = 0;
            for (double number : numbers) total += number;
            return {total, {}};
        }
        if (name == QStringLiteral("AVERAGE")) {
            if (numbers.isEmpty()) return {0, QStringLiteral("#DIV/0!")};
            double total = 0;
            for (double number : numbers) total += number;
            return {total / numbers.size(), {}};
        }
        if (name == QStringLiteral("MIN") || name == QStringLiteral("MAX")) {
            if (numbers.isEmpty()) return {0, QStringLiteral("#ERROR!")};
            double result = numbers.first();
            for (double number : numbers)
                result = name == QStringLiteral("MIN") ? qMin(result, number) : qMax(result, number);
            return {result, {}};
        }
        if(name==QStringLiteral("PRODUCT")){
            if(numbers.isEmpty())return {0,{}};
            double result=1;for(double number:numbers)result*=number;return {result,{}};
        }
        if(name==QStringLiteral("MEDIAN")){
            if(numbers.isEmpty())return {0,QStringLiteral("#ERROR!")};
            std::sort(numbers.begin(),numbers.end());const int middle=numbers.size()/2;
            return {numbers.size()%2?numbers[middle]:(numbers[middle-1]+numbers[middle])/2,{}};
        }
        if(name==QStringLiteral("ABS")||name==QStringLiteral("SQRT")){
            if(values.size()!=1)return {0,QStringLiteral("#ERROR!")};
            if(name==QStringLiteral("SQRT")&&values[0].number<0)return {0,QStringLiteral("#NUM!")};
            return {name==QStringLiteral("ABS")?std::abs(values[0].number):std::sqrt(values[0].number),{}};
        }
        if(name==QStringLiteral("POWER")){
            if(values.size()!=2)return {0,QStringLiteral("#ERROR!")};
            return {std::pow(values[0].number,values[1].number),{}};
        }
        if(name==QStringLiteral("ROUND")){
            if(values.size()!=2)return {0,QStringLiteral("#ERROR!")};
            if(!std::isfinite(values[1].number))return {0,QStringLiteral("#ERROR!")};
            const int digits=values[1].number < -10 ? -10 : values[1].number > 10 ? 10 : int(std::trunc(values[1].number));
            const double scale=std::pow(10.0,digits);
            return {std::round(values[0].number*scale)/scale,{}};
        }
        return {0, QStringLiteral("#NAME?")};
    }
    Value factor() {
        spaces();
        if (take('+')) return factor();
        if (take('-')) {
            Value value = factor();
            value.number = -value.number;
            value.numeric = value.populated = true;
            return value;
        }
        if (take('(')) {
            Value value = expression();
            if (!take(')') && value.error.isEmpty()) value.error = QStringLiteral("#ERROR!");
            return value;
        }
        const int start = m_position;
        if (m_position < m_expression.size() && m_expression.at(m_position).isLetter()) {
            while (m_position < m_expression.size() && m_expression.at(m_position).isLetter()) ++m_position;
            const QString name = m_expression.mid(start, m_position - start).toUpper();
            if (take('(')) return function(name);
            m_position = start;
            int row = -1, column = -1;
            if (!reference(row, column)) return {0, QStringLiteral("#NAME?")};
            return m_cell(row, column);
        }
        bool digit = false;
        while (m_position < m_expression.size() &&
               (m_expression.at(m_position).isDigit() || m_expression.at(m_position) == '.')) {
            digit = true;
            ++m_position;
        }
        if (!digit) return {0, QStringLiteral("#ERROR!")};
        bool ok = false;
        const double number = m_expression.mid(start, m_position - start).toDouble(&ok);
        return ok ? Value{number, {}} : Value{0, QStringLiteral("#ERROR!")};
    }

    QString m_expression;
    std::function<Value(int, int)> m_cell;
    int m_position = 0;
};

bool parseCsv(const QString &text, QList<QStringList> &rows) {
    QString field;
    QStringList row;
    bool quoted = false;
    bool afterQuote = false;
    for (int index = 0; index < text.size(); ++index) {
        const QChar character = text.at(index);
        if (quoted) {
            if (character == '"') {
                if (index + 1 < text.size() && text.at(index + 1) == '"') {
                    field += '"';
                    ++index;
                } else { quoted = false; afterQuote = true; }
            } else field += character;
        } else if (character == '"' && field.isEmpty() && !afterQuote) {
            quoted = true;
        } else if (character == ',' || character == '\n' || character == '\r') {
            row.append(field);
            field.clear();
            afterQuote = false;
            if (character != ',') {
                rows.append(row);
                row.clear();
                if (character == '\r' && index + 1 < text.size() && text.at(index + 1) == '\n') ++index;
            }
        } else {
            if (afterQuote) return false;
            field += character;
        }
    }
    if (quoted) return false;
    if (!field.isEmpty() || !row.isEmpty() || afterQuote) {
        row.append(field);
        rows.append(row);
    }
    return true;
}

QString csvField(const QString &field) {
    if (!field.contains(',') && !field.contains('"') && !field.contains('\n') && !field.contains('\r'))
        return field;
    QString escaped = field;
    escaped.replace('"', QStringLiteral("\"\""));
    return QStringLiteral("\"") + escaped + QStringLiteral("\"");
}
}

SheetsDocument::SheetsDocument(QObject *parent) : QAbstractTableModel(parent) {
    m_recoveryTimer.setInterval(15000);
    connect(&m_recoveryTimer, &QTimer::timeout, this, &SheetsDocument::writeRecoveryNow);
    m_sessionRecovery = recoveryDirectory() + QLatin1Char('/') + QUuid::createUuid().toString(QUuid::WithoutBraces) + QStringLiteral(".json");
    findRecovery();
    m_viewRows.reserve(rows);
    for (int row = 0; row < rows; ++row) m_viewRows.append(row);
}

int SheetsDocument::rowCount(const QModelIndex &parent) const { return parent.isValid() ? 0 : m_viewRows.size(); }
int SheetsDocument::columnCount(const QModelIndex &parent) const { return parent.isValid() ? 0 : columns; }
QHash<int, QByteArray> SheetsDocument::roleNames() const {
    return {{Qt::DisplayRole, "display"}, {Qt::UserRole + 1, "raw"},
            {Qt::UserRole + 2, "bold"}, {Qt::UserRole + 3, "sourceRow"}};
}
QVariant SheetsDocument::data(const QModelIndex &index, int role) const {
    if (!index.isValid()) return {};
    const int source = sourceRow(index.row());
    if (role == Qt::DisplayRole) return displayAt(source, index.column());
    if (role == Qt::UserRole + 1) return rawAt(source, index.column());
    if (role == Qt::UserRole + 2) return boldAt(source, index.column());
    if (role == Qt::UserRole + 3) return source;
    return {};
}
QString SheetsDocument::displayName() const {
    return m_path.isEmpty() ? QStringLiteral("Untitled spreadsheet") : QFileInfo(m_path).fileName();
}
QString SheetsDocument::columnName(int column) const {
    if (column < 0 || column >= columns) return {};
    QString name;
    do {
        name.prepend(QChar('A' + column % 26));
        column = column / 26 - 1;
    } while (column >= 0);
    return name;
}
QString SheetsDocument::cellName(int row, int column) const {
    if (row < 0 || row >= rows) return {};
    const QString label = columnName(column);
    return label.isEmpty() ? QString() : label + QString::number(row + 1);
}
QString SheetsDocument::rawAt(int row, int column) const {
    if (row < 0 || row >= rows || column < 0 || column >= columns) return {};
    return m_cells.value(key(row, column));
}
QString SheetsDocument::evaluate(int row, int column, QSet<int> &visiting) const {
    if (row < 0 || row >= rows || column < 0 || column >= columns) return QStringLiteral("#REF!");
    const int address = key(row, column);
    const QString raw = m_cells.value(address);
    if (!raw.startsWith('=')) return raw;
    if (visiting.contains(address)) return QStringLiteral("#CYCLE!");
    visiting.insert(address);
    FormulaParser parser(raw.mid(1), [this, &visiting](int referenceRow, int referenceColumn) -> Value {
        const QString result = evaluate(referenceRow, referenceColumn, visiting);
        if (result.startsWith('#')) return {0, result};
        bool ok = false;
        const double number = result.toDouble(&ok);
        return {ok ? number : 0, {}, ok, !result.isEmpty()};
    });
    const Value result = parser.parse();
    visiting.remove(address);
    if (!result.error.isEmpty()) return result.error;
    if (!std::isfinite(result.number)) return QStringLiteral("#ERROR!");
    return QString::number(result.number, 'g', 12);
}
QString SheetsDocument::displayAt(int row, int column) const {
    QSet<int> visiting;
    const QString value = evaluate(row, column, visiting);
    if (value.startsWith('#')) return value;
    const int format = numberFormatAt(row, column);
    if (format == 0 || value.isEmpty()) return value;
    bool ok = false;
    const double number = value.toDouble(&ok);
    if (!ok) return value;
    if (format == 1) return QString::number(number, 'f', 2);
    if (format == 2) return QStringLiteral("$") + QString::number(number, 'f', 2);
    if (format == 3) return QString::number(number * 100, 'f', 1) + QStringLiteral("%");
    return value;
}
void SheetsDocument::setError(const QString &error) {
    if (m_error == error) return;
    m_error = error;
    emit errorChanged();
}
void SheetsDocument::refresh() {
    ++m_revision;
    if (!m_viewRows.isEmpty())
        emit dataChanged(index(0, 0), index(m_viewRows.size() - 1, columns - 1),
                         {Qt::DisplayRole, Qt::UserRole + 1, Qt::UserRole + 2, Qt::UserRole + 3});
    emit stateChanged();
}
void SheetsDocument::newDocument() {
    beginResetModel();
    m_sheets = {SheetState()};
    m_activeSheet = 0;
    m_cells.clear();
    m_numberFormats.clear();
    m_boldCells.clear();
    m_rowHeights.clear();
    m_columnWidths.clear();
    m_sortColumn = -1;
    m_filterColumn = -1;
    m_filterQuery.clear();
    m_chartRange.clear();
    m_chartType = QStringLiteral("bar");
    m_viewRows.clear();
    for (int row = 0; row < rows; ++row) m_viewRows.append(row);
    endResetModel();
    m_path.clear();
    m_dirty = false;
    m_hasDocument = true;
    clearHistory();
    m_savedState = QJsonDocument(nativeObject()).toJson(QJsonDocument::Compact);
    removeSessionRecovery();
    setError({});
    refresh();
    emit dimensionsChanged();
    emit sheetsChanged();
}
void SheetsDocument::setCell(int row, int column, const QString &raw) {
    if (row < 0 || row >= rows || column < 0 || column >= columns) return;
    const int address = key(row, column);
    if (m_cells.value(address) == raw) return;
    recordEdit();
    if (raw.isEmpty()) m_cells.remove(address);
    else m_cells.insert(address, raw);
    m_dirty = true;
    m_hasDocument = true;
    if (m_sortColumn >= 0 || !m_filterQuery.isEmpty()) rebuildView();
    refresh();
}
bool SheetsDocument::open(const QUrl &url) {
    const QString path = url.isLocalFile() ? url.toLocalFile() : QString();
    if (path.endsWith(QStringLiteral(".nsheets"), Qt::CaseInsensitive)) return openNative(path);
    return openCsv(path);
}
bool SheetsDocument::openCsv(const QString &path) {
    QFile file(path);
    if (path.isEmpty() || !file.open(QIODevice::ReadOnly) || file.size() > 5 * 1024 * 1024) {
        setError(QStringLiteral("Could not open this CSV file (maximum 5 MB)."));
        return false;
    }
    QString content = QString::fromUtf8(file.readAll());
    if (content.startsWith(QChar::ByteOrderMark)) content.remove(0, 1);
    QList<QStringList> parsed;
    if (!parseCsv(content, parsed) || parsed.size() > rows) {
        setError(QStringLiteral("Invalid CSV or more than 500 rows."));
        return false;
    }
    QHash<int, QString> cells;
    for (int row = 0; row < parsed.size(); ++row) {
        if (parsed.at(row).size() > columns) {
            setError(QStringLiteral("This CSV has more than 52 columns."));
            return false;
        }
        for (int column = 0; column < parsed.at(row).size(); ++column)
            if (!parsed.at(row).at(column).isEmpty()) cells.insert(key(row, column), parsed.at(row).at(column));
    }
    beginResetModel();
    m_cells = std::move(cells);
    m_sheets = {SheetState()};
    m_activeSheet = 0;
    m_numberFormats.clear();
    m_boldCells.clear();
    m_rowHeights.clear();
    m_columnWidths.clear();
    m_sortColumn = -1;
    m_filterColumn = -1;
    m_filterQuery.clear();
    m_chartRange.clear();
    m_chartType = QStringLiteral("bar");
    m_viewRows.clear();
    for (int row = 0; row < rows; ++row) m_viewRows.append(row);
    endResetModel();
    m_path = QFileInfo(path).absoluteFilePath();
    m_dirty = false;
    m_hasDocument = true;
    clearHistory();
    m_savedState = QJsonDocument(nativeObject()).toJson(QJsonDocument::Compact);
    removeSessionRecovery();
    setError({});
    refresh();
    emit dimensionsChanged();
    emit sheetsChanged();
    return true;
}
bool SheetsDocument::save() {
    if (m_path.isEmpty()) {
        setError(QStringLiteral("Choose a spreadsheet file name first."));
        return false;
    }
    return saveAs(QUrl::fromLocalFile(m_path));
}
bool SheetsDocument::saveAs(const QUrl &url) {
    QString path = url.isLocalFile() ? url.toLocalFile() : QString();
    if (path.isEmpty()) { setError(QStringLiteral("Choose a local save destination.")); return false; }
    if (path.endsWith(QStringLiteral(".csv"), Qt::CaseInsensitive)) return saveCsv(path);
    if (!path.endsWith(QStringLiteral(".nsheets"), Qt::CaseInsensitive)) path += QStringLiteral(".nsheets");
    return saveNative(path);
}
bool SheetsDocument::saveCsv(const QString &path) {
    if (hasNativeFeatures() || m_sheets.size() > 1) {
        setError(QStringLiteral("Use Save As and choose a .nsheets file to keep formatting and charts."));
        return false;
    }
    if (!exportCsv(QUrl::fromLocalFile(path))) return false;
    m_path = QFileInfo(path).absoluteFilePath();
    m_dirty = false;
    m_savedState = QJsonDocument(nativeObject()).toJson(QJsonDocument::Compact);
    removeSessionRecovery();
    emit stateChanged();
    return true;
}

bool SheetsDocument::exportCsv(const QUrl &url) {
    QString path = url.isLocalFile() ? url.toLocalFile() : QString();
    if (path.isEmpty()) { setError(QStringLiteral("Choose a local CSV destination.")); return false; }
    if (!path.endsWith(QStringLiteral(".csv"), Qt::CaseInsensitive)) path += QStringLiteral(".csv");
    int lastRow = -1;
    int lastColumn = -1;
    for (auto it = m_cells.cbegin(); it != m_cells.cend(); ++it) {
        lastRow = qMax(lastRow, it.key() / columns);
        lastColumn = qMax(lastColumn, it.key() % columns);
    }
    QStringList lines;
    for (int row = 0; row <= lastRow; ++row) {
        QStringList fields;
        for (int column = 0; column <= lastColumn; ++column)
            fields.append(csvField(rawAt(row, column)));
        lines.append(fields.join(','));
    }
    const QByteArray data = (lines.join(QStringLiteral("\r\n")) + (lines.isEmpty() ? QString() : QStringLiteral("\r\n"))).toUtf8();
    QSaveFile file(path);
    if (!file.open(QIODevice::WriteOnly) || file.write(data) != data.size() || !file.commit()) {
        setError(QStringLiteral("Could not save the CSV file."));
        return false;
    }
    setError({});
    return true;
}
