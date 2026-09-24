#include "SheetsDocument.h"

#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QPageLayout>
#include <QPageSize>
#include <QPainter>
#include <QPainterPath>
#include <QPdfWriter>
#include <QRegularExpression>
#include <QSaveFile>
#include <algorithm>

namespace {
bool parseAddress(const QString &name, int &row, int &column) {
    static const QRegularExpression pattern(QStringLiteral("^([A-Za-z]{1,2})([1-9][0-9]*)$"));
    const auto match = pattern.match(name);
    if (!match.hasMatch()) return false;
    const QString letters = match.captured(1).toUpper();
    int parsedColumn = 0;
    for (const QChar letter : letters) parsedColumn = parsedColumn * 26 + letter.unicode() - 'A' + 1;
    bool ok = false;
    const int parsedRow = match.captured(2).toInt(&ok);
    row = parsedRow - 1;
    column = parsedColumn - 1;
    return ok && row >= 0 && row < 500 && column >= 0 && column < 52;
}
bool parseRange(const QString &range, int &firstRow, int &firstColumn, int &lastRow, int &lastColumn) {
    const QStringList parts = range.split(':');
    return parts.size() == 2 &&
           parseAddress(parts.at(0), firstRow, firstColumn) &&
           parseAddress(parts.at(1), lastRow, lastColumn) &&
           lastRow >= firstRow && lastColumn >= firstColumn &&
           lastRow - firstRow < 30 && lastColumn - firstColumn <= 1;
}
}

int SheetsDocument::sourceRow(int viewRow) const {
    return viewRow >= 0 && viewRow < m_viewRows.size() ? m_viewRows.at(viewRow) : -1;
}

int SheetsDocument::numberFormatAt(int row, int column) const {
    if (row < 0 || row >= rows || column < 0 || column >= columns) return 0;
    return m_numberFormats.value(key(row, column), 0);
}

bool SheetsDocument::boldAt(int row, int column) const {
    if (row < 0 || row >= rows || column < 0 || column >= columns) return false;
    return m_boldCells.contains(key(row, column));
}

QString SheetsDocument::fillColorAt(int row, int column) const {
    return row >= 0 && row < rows && column >= 0 && column < columns
        ? m_fillColors.value(key(row, column)) : QString();
}

QString SheetsDocument::textColorAt(int row, int column) const {
    return row >= 0 && row < rows && column >= 0 && column < columns
        ? m_textColors.value(key(row, column)) : QString();
}

int SheetsDocument::alignmentAt(int row, int column) const {
    return row >= 0 && row < rows && column >= 0 && column < columns
        ? m_alignments.value(key(row, column)) : 0;
}

void SheetsDocument::setNumberFormat(int row, int column, int format) {
    setRangeNumberFormat(row, column, row, column, format);
}

void SheetsDocument::toggleBold(int row, int column) {
    toggleRangeBold(row, column, row, column);
}

void SheetsDocument::setRangeNumberFormat(int firstRow, int firstColumn, int lastRow, int lastColumn, int format) {
    const int top = std::min(firstRow, lastRow), bottom = std::max(firstRow, lastRow);
    const int left = std::min(firstColumn, lastColumn), right = std::max(firstColumn, lastColumn);
    if (top < 0 || bottom >= rows || left < 0 || right >= columns || format < 0 || format > 3) return;
    bool changed = false;
    for (int row = top; row <= bottom; ++row) for (int column = left; column <= right; ++column)
        changed |= numberFormatAt(row, column) != format;
    if (!changed) return;
    recordEdit();
    for (int row = top; row <= bottom; ++row) for (int column = left; column <= right; ++column) {
        const int address = key(row, column);
        if (format) m_numberFormats.insert(address, format); else m_numberFormats.remove(address);
    }
    m_dirty = m_hasDocument = true;
    refresh();
}

void SheetsDocument::toggleRangeBold(int firstRow, int firstColumn, int lastRow, int lastColumn) {
    const int top = std::min(firstRow, lastRow), bottom = std::max(firstRow, lastRow);
    const int left = std::min(firstColumn, lastColumn), right = std::max(firstColumn, lastColumn);
    if (top < 0 || bottom >= rows || left < 0 || right >= columns) return;
    const bool makeBold = !boldAt(firstRow, firstColumn);
    recordEdit();
    for (int row = top; row <= bottom; ++row) for (int column = left; column <= right; ++column) {
        const int address = key(row, column);
        if (makeBold) m_boldCells.insert(address); else m_boldCells.remove(address);
    }
    m_dirty = m_hasDocument = true;
    refresh();
}

void SheetsDocument::setRangeFillColor(int firstRow, int firstColumn, int lastRow, int lastColumn, const QString &color) {
    const int top = std::min(firstRow, lastRow), bottom = std::max(firstRow, lastRow);
    const int left = std::min(firstColumn, lastColumn), right = std::max(firstColumn, lastColumn);
    const QColor chosen(color);
    if (top < 0 || bottom >= rows || left < 0 || right >= columns || (!color.isEmpty() && !chosen.isValid())) return;
    const QString normalized = color.isEmpty() ? QString() : chosen.name(QColor::HexRgb);
    bool changed = false;
    for (int row = top; row <= bottom; ++row) for (int column = left; column <= right; ++column)
        changed |= fillColorAt(row, column) != normalized;
    if (!changed) return;
    recordEdit();
    for (int row = top; row <= bottom; ++row) for (int column = left; column <= right; ++column) {
        const int address = key(row, column);
        if (normalized.isEmpty()) m_fillColors.remove(address); else m_fillColors.insert(address, normalized);
    }
    m_dirty = m_hasDocument = true;
    refresh();
}

void SheetsDocument::setRangeTextColor(int firstRow, int firstColumn, int lastRow, int lastColumn, const QString &color) {
    const int top = std::min(firstRow, lastRow), bottom = std::max(firstRow, lastRow);
    const int left = std::min(firstColumn, lastColumn), right = std::max(firstColumn, lastColumn);
    const QColor chosen(color);
    if (top < 0 || bottom >= rows || left < 0 || right >= columns || (!color.isEmpty() && !chosen.isValid())) return;
    const QString normalized = color.isEmpty() ? QString() : chosen.name(QColor::HexRgb);
    bool changed = false;
    for (int row = top; row <= bottom; ++row) for (int column = left; column <= right; ++column)
        changed |= textColorAt(row, column) != normalized;
    if (!changed) return;
    recordEdit();
    for (int row = top; row <= bottom; ++row) for (int column = left; column <= right; ++column) {
        const int address = key(row, column);
        if (normalized.isEmpty()) m_textColors.remove(address); else m_textColors.insert(address, normalized);
    }
    m_dirty = m_hasDocument = true;
    refresh();
}

void SheetsDocument::setRangeAlignment(int firstRow, int firstColumn, int lastRow, int lastColumn, int alignment) {
    const int top = std::min(firstRow, lastRow), bottom = std::max(firstRow, lastRow);
    const int left = std::min(firstColumn, lastColumn), right = std::max(firstColumn, lastColumn);
    if (top < 0 || bottom >= rows || left < 0 || right >= columns || alignment < 0 || alignment > 2) return;
    bool changed = false;
    for (int row = top; row <= bottom; ++row) for (int column = left; column <= right; ++column)
        changed |= alignmentAt(row, column) != alignment;
    if (!changed) return;
    recordEdit();
    for (int row = top; row <= bottom; ++row) for (int column = left; column <= right; ++column) {
        const int address = key(row, column);
        if (alignment) m_alignments.insert(address, alignment); else m_alignments.remove(address);
    }
    m_dirty = m_hasDocument = true;
    refresh();
}

int SheetsDocument::rowHeight(int row) const {
    return row >= 0 && row < rows ? m_rowHeights.value(row, 30) : 30;
}
int SheetsDocument::columnWidth(int column) const {
    return column >= 0 && column < columns ? m_columnWidths.value(column, 112) : 112;
}
void SheetsDocument::setRowHeight(int row, int height) {
    if (row < 0 || row >= rows) return;
    height = qBound(20, height, 240);
    if (rowHeight(row) == height) return;
    recordEdit();
    if (height == 30) m_rowHeights.remove(row); else m_rowHeights.insert(row, height);
    m_dirty = true;
    emit dimensionsChanged();
    emit stateChanged();
}
void SheetsDocument::setColumnWidth(int column, int width) {
    if (column < 0 || column >= columns) return;
    width = qBound(48, width, 480);
    if (columnWidth(column) == width) return;
    recordEdit();
    if (width == 112) m_columnWidths.remove(column); else m_columnWidths.insert(column, width);
    m_dirty = true;
    emit dimensionsChanged();
    emit stateChanged();
}

void SheetsDocument::rebuildView() {
    beginResetModel();
    m_viewRows.clear();
    for (int row = 0; row < rows; ++row) {
        if (!m_filterQuery.isEmpty() &&
            !displayAt(row, m_filterColumn).contains(m_filterQuery, Qt::CaseInsensitive)) continue;
        m_viewRows.append(row);
    }
    if (m_sortColumn >= 0) {
        std::stable_sort(m_viewRows.begin(), m_viewRows.end(), [this](int leftRow, int rightRow) {
            if (leftRow == 0 || rightRow == 0) return leftRow == 0;
            QSet<int> leftVisiting, rightVisiting;
            const QString left = evaluate(leftRow, m_sortColumn, leftVisiting);
            const QString right = evaluate(rightRow, m_sortColumn, rightVisiting);
            if (left.isEmpty() != right.isEmpty()) return !left.isEmpty();
            bool leftNumber = false, rightNumber = false;
            const double a = left.toDouble(&leftNumber);
            const double b = right.toDouble(&rightNumber);
            if (leftNumber != rightNumber) return leftNumber;
            const int comparison = leftNumber && rightNumber ? (a < b ? -1 : a > b ? 1 : 0)
                : QString::localeAwareCompare(left, right);
            return m_sortAscending ? comparison < 0 : comparison > 0;
        });
    }
    endResetModel();
}

void SheetsDocument::sortBy(int column, bool ascending) {
    if (column < 0 || column >= columns) return;
    if (m_sortColumn == column && m_sortAscending == ascending) return;
    recordEdit();
    m_sortColumn = column;
    m_sortAscending = ascending;
    if (m_path.isEmpty() || m_path.endsWith(QStringLiteral(".nsheets"), Qt::CaseInsensitive)) m_dirty = true;
    rebuildView();
    refresh();
}

void SheetsDocument::clearSort() {
    if (m_sortColumn < 0) return;
    recordEdit();
    m_sortColumn = -1;
    if (m_path.isEmpty() || m_path.endsWith(QStringLiteral(".nsheets"), Qt::CaseInsensitive)) m_dirty = true;
    rebuildView();
    refresh();
}

void SheetsDocument::setFilter(int column, const QString &query) {
    if (column < 0 || column >= columns) return;
    if (m_filterColumn == column && m_filterQuery == query.trimmed()) return;
    recordEdit();
    m_filterColumn = column;
    m_filterQuery = query.trimmed();
    if (m_path.isEmpty() || m_path.endsWith(QStringLiteral(".nsheets"), Qt::CaseInsensitive)) m_dirty = true;
    rebuildView();
    refresh();
}

void SheetsDocument::setChart(const QString &range, const QString &type) {
    if(type!="bar"&&type!="line"&&type!="area"&&type!="pie"){
        setError(QStringLiteral("Choose bar, line, area, or pie chart."));return;
    }
    if (!range.isEmpty()) {
        int firstRow, firstColumn, lastRow, lastColumn;
        if (!parseRange(range.toUpper(), firstRow, firstColumn, lastRow, lastColumn)) {
            setError(QStringLiteral("Choose a range of up to 30 rows and two columns, such as A1:B8."));
            return;
        }
    }
    if (m_chartRange == range.toUpper() && m_chartType == type) return;
    recordEdit();
    m_chartRange = range.toUpper();
    m_chartType = type;
    m_dirty = true;
    setError({});
    refresh();
}

QVariantList SheetsDocument::chartData() const {
    QVariantList points;
    int firstRow, firstColumn, lastRow, lastColumn;
    if (!parseRange(m_chartRange, firstRow, firstColumn, lastRow, lastColumn)) return points;
    for (int row = firstRow; row <= lastRow; ++row) {
        const int valueColumn = lastColumn;
        QSet<int> visiting;
        const QString rawValue = evaluate(row, valueColumn, visiting);
        bool valid = false;
        const double value = rawValue.toDouble(&valid);
        if (!valid) continue;
        const QString label = firstColumn == lastColumn ? QString::number(row + 1)
            : rawAt(row, firstColumn);
        points.append(QVariantMap{{QStringLiteral("label"), label.isEmpty() ? QString::number(row + 1) : label},
                                  {QStringLiteral("value"), value}});
    }
    return points;
}

bool SheetsDocument::hasNativeFeatures() const {
    return m_sheets.size() > 1 || m_sheets[m_activeSheet].name != QStringLiteral("Sheet 1") ||
           !m_numberFormats.isEmpty() || !m_boldCells.isEmpty() || !m_alignments.isEmpty() ||
           !m_fillColors.isEmpty() || !m_textColors.isEmpty() || !m_chartRange.isEmpty() ||
           !m_rowHeights.isEmpty() || !m_columnWidths.isEmpty();
}

QJsonObject SheetsDocument::sheetObject(const SheetState &sheet) const {
    QSet<int> addresses;
    for (auto it = sheet.cells.cbegin(); it != sheet.cells.cend(); ++it) addresses.insert(it.key());
    for (auto it = sheet.numberFormats.cbegin(); it != sheet.numberFormats.cend(); ++it) addresses.insert(it.key());
    for (auto it = sheet.alignments.cbegin(); it != sheet.alignments.cend(); ++it) addresses.insert(it.key());
    for (auto it = sheet.fillColors.cbegin(); it != sheet.fillColors.cend(); ++it) addresses.insert(it.key());
    for (auto it = sheet.textColors.cbegin(); it != sheet.textColors.cend(); ++it) addresses.insert(it.key());
    addresses.unite(sheet.boldCells);
    QList<int> sorted = addresses.values();
    std::sort(sorted.begin(), sorted.end());
    QJsonArray cells;
    for (const int address : sorted) {
        const int row = address / columns;
        const int column = address % columns;
        cells.append(QJsonObject{{QStringLiteral("row"), row},
                                 {QStringLiteral("column"), column},
                                 {QStringLiteral("raw"), sheet.cells.value(address)},
                                 {QStringLiteral("numberFormat"), sheet.numberFormats.value(address)},
                                 {QStringLiteral("alignment"), sheet.alignments.value(address)},
                                 {QStringLiteral("fillColor"), sheet.fillColors.value(address)},
                                 {QStringLiteral("textColor"), sheet.textColors.value(address)},
                                 {QStringLiteral("bold"), sheet.boldCells.contains(address)}});
    }
    QJsonArray rowHeights, columnWidths;
    QList<int> rowKeys = sheet.rowHeights.keys();
    QList<int> columnKeys = sheet.columnWidths.keys();
    std::sort(rowKeys.begin(), rowKeys.end());
    std::sort(columnKeys.begin(), columnKeys.end());
    for (int row : rowKeys) rowHeights.append(QJsonArray{row, sheet.rowHeights.value(row)});
    for (int column : columnKeys) columnWidths.append(QJsonArray{column, sheet.columnWidths.value(column)});
    return QJsonObject{{QStringLiteral("format"), QStringLiteral("nexus-sheets")},
                             {QStringLiteral("version"), 1},
                             {QStringLiteral("name"), sheet.name},
                             {QStringLiteral("cells"), cells},
                             {QStringLiteral("rowHeights"), rowHeights},
                             {QStringLiteral("columnWidths"), columnWidths},
                             {QStringLiteral("sortColumn"), sheet.sortColumn},
                             {QStringLiteral("sortAscending"), sheet.sortAscending},
                             {QStringLiteral("filterColumn"), sheet.filterColumn},
                             {QStringLiteral("filterQuery"), sheet.filterQuery},
                             {QStringLiteral("chartRange"), sheet.chartRange},
                             {QStringLiteral("chartType"), sheet.chartType}};
}

QJsonObject SheetsDocument::nativeObject() const {
    const QVector<SheetState> sheets=allSheets();
    if(sheets.size()==1)return sheetObject(sheets.first());
    QJsonArray items;for(const auto &sheet:sheets)items.append(sheetObject(sheet));
    return QJsonObject{{"format","nexus-sheets"},{"version",2},{"activeSheet",m_activeSheet},{"sheets",items}};
}

bool SheetsDocument::saveNative(const QString &path) {
    const QByteArray data = QJsonDocument(nativeObject()).toJson(QJsonDocument::Compact);
    if (data.size() > 15 * 1024 * 1024) {
        setError(QStringLiteral("This sheet is too large for a Nexus Sheets file."));
        return false;
    }
    QSaveFile file(path);
    if (!file.open(QIODevice::WriteOnly) || file.write(data) != data.size() || !file.commit()) {
        setError(QStringLiteral("Could not save the Nexus Sheets file."));
        return false;
    }
    m_path = QFileInfo(path).absoluteFilePath();
    m_dirty = false;
    m_savedState = data;
    removeSessionRecovery();
    setError({});
    emit stateChanged();
    return true;
}

bool SheetsDocument::parseSheetObject(const QJsonObject &object, SheetState &sheet) {
    const QJsonArray cells=object.value("cells").toArray();
    if(cells.size()>rows*columns)return false;
    sheet.name=object.value("name").toString("Sheet 1");
    if(sheet.name.trimmed().isEmpty()||sheet.name.size()>31)return false;
    for(const QJsonValue &value:cells){
        if(!value.isObject())return false;
        QJsonObject cell=value.toObject();int row=cell.value("row").toInt(-1),column=cell.value("column").toInt(-1);
        int format=cell.value("numberFormat").toInt(), alignment=cell.value("alignment").toInt();
        QString raw=cell.value("raw").toString();
        QString fill=cell.value("fillColor").toString(), textColor=cell.value("textColor").toString();
        if(row<0||row>=rows||column<0||column>=columns||format<0||format>3||alignment<0||alignment>2||
           raw.size()>10000||(!fill.isEmpty()&&!QColor(fill).isValid())||
           (!textColor.isEmpty()&&!QColor(textColor).isValid()))return false;
        int address=key(row,column);if(!raw.isEmpty())sheet.cells.insert(address,raw);
        if(format)sheet.numberFormats.insert(address,format);if(cell.value("bold").toBool())sheet.boldCells.insert(address);
        if(alignment)sheet.alignments.insert(address,alignment);
        if(!fill.isEmpty())sheet.fillColors.insert(address,QColor(fill).name(QColor::HexRgb));
        if(!textColor.isEmpty())sheet.textColors.insert(address,QColor(textColor).name(QColor::HexRgb));
    }
    const auto sizes=[](const QJsonArray &array,int count,int minimum,int maximum,QHash<int,int> &out){
        for(const QJsonValue &v:array){QJsonArray pair=v.toArray();if(pair.size()!=2||!pair[0].isDouble()||!pair[1].isDouble())return false;
            int index=pair[0].toInt(-1),size=pair[1].toInt(-1);if(index<0||index>=count||size<minimum||size>maximum)return false;out.insert(index,size);}
        return true;
    };
    if(!sizes(object.value("rowHeights").toArray(),rows,20,240,sheet.rowHeights)||
       !sizes(object.value("columnWidths").toArray(),columns,48,480,sheet.columnWidths))return false;
    sheet.sortColumn=object.value("sortColumn").toInt(-1);if(sheet.sortColumn<-1||sheet.sortColumn>=columns)sheet.sortColumn=-1;
    sheet.sortAscending=object.value("sortAscending").toBool(true);
    sheet.filterColumn=object.value("filterColumn").toInt(-1);if(sheet.filterColumn<-1||sheet.filterColumn>=columns)sheet.filterColumn=-1;
    sheet.filterQuery=object.value("filterQuery").toString();if(sheet.filterColumn<0)sheet.filterQuery.clear();
    sheet.chartRange=object.value("chartRange").toString();
    int firstRow,firstColumn,lastRow,lastColumn;
    if(!sheet.chartRange.isEmpty()&&!parseRange(sheet.chartRange,firstRow,firstColumn,lastRow,lastColumn))return false;
    sheet.chartType=object.value("chartType").toString("bar");
    if(sheet.chartType.isEmpty())sheet.chartType="bar";
    if(sheet.chartType!="bar"&&sheet.chartType!="line"&&sheet.chartType!="area"&&sheet.chartType!="pie")return false;
    for(int row=0;row<rows;++row)sheet.viewRows.append(row);
    return true;
}
bool SheetsDocument::openNative(const QString &path) {
    QFile file(path);
    if(!file.open(QIODevice::ReadOnly)||file.size()>15*1024*1024){setError("Could not open this Nexus Sheets file.");return false;}
    QJsonParseError error;QJsonDocument parsed=QJsonDocument::fromJson(file.readAll(),&error);QJsonObject object=parsed.object();
    if(error.error!=QJsonParseError::NoError||object.value("format")!="nexus-sheets"){
        setError("This is not a supported Nexus Sheets file.");return false;}
    int version=object.value("version").toInt();QVector<SheetState> sheets;int active=0;
    if(version==1){SheetState sheet;if(!parseSheetObject(object,sheet)){setError("This sheet contains invalid data.");return false;}sheets.append(sheet);}
    else if(version==2){QJsonArray entries=object.value("sheets").toArray();active=object.value("activeSheet").toInt(-1);
        if(entries.isEmpty()||entries.size()>20||active<0||active>=entries.size()){setError("This workbook has invalid sheets.");return false;}
        QSet<QString> names;for(const QJsonValue &entry:entries){SheetState sheet;if(!entry.isObject()||!parseSheetObject(entry.toObject(),sheet)||names.contains(sheet.name.toCaseFolded())){setError("This workbook has invalid sheets.");return false;}names.insert(sheet.name.toCaseFolded());sheets.append(sheet);}}
    else{setError("This is not a supported Nexus Sheets file.");return false;}
    beginResetModel();m_sheets=std::move(sheets);m_activeSheet=active;loadActiveState(m_sheets[active]);endResetModel();
    if(m_sortColumn>=0||!m_filterQuery.isEmpty())rebuildView();
    m_path=QFileInfo(path).absoluteFilePath();m_dirty=false;m_hasDocument=true;clearHistory();
    m_savedState=QJsonDocument(nativeObject()).toJson(QJsonDocument::Compact);removeSessionRecovery();setError({});
    refresh();emit dimensionsChanged();emit sheetsChanged();return true;
}

bool SheetsDocument::exportPdf(const QUrl &url) {
    QString path = url.isLocalFile() ? url.toLocalFile() : QString();
    if (path.isEmpty()) { setError(QStringLiteral("Choose a local PDF destination.")); return false; }
    if (!path.endsWith(QStringLiteral(".pdf"), Qt::CaseInsensitive)) path += QStringLiteral(".pdf");
    QSaveFile file(path);
    if (!file.open(QIODevice::WriteOnly)) { setError(QStringLiteral("Could not create the PDF.")); return false; }
    {
        QPdfWriter writer(&file);
        writer.setPageSize(QPageSize(QPageSize::A4));
        writer.setPageOrientation(QPageLayout::Landscape);
        writer.setResolution(96);
        QPainter painter(&writer);
        painter.setRenderHint(QPainter::Antialiasing);
        const int margin = 46;
        const int cellWidth = 125;
        const int rowHeight = 22;
        int lastColumn = 0;
        for (auto it = m_cells.cbegin(); it != m_cells.cend(); ++it)
            lastColumn = qMax(lastColumn, it.key() % columns);
        QVector<int> usedRows;
        for (int row : m_viewRows) {
            bool used = false;
            for (int column = 0; column <= lastColumn; ++column)
                if (!rawAt(row, column).isEmpty()) { used = true; break; }
            if (used) usedRows.append(row);
        }
        if (usedRows.isEmpty()) usedRows.append(0);
        const int pagesAcross = lastColumn / 8 + 1;
        const int pagesDown = (usedRows.size() + 27) / 28;
        bool firstPage = true;
        for (int horizontal = 0; horizontal < pagesAcross; ++horizontal) {
            for (int vertical = 0; vertical < pagesDown; ++vertical) {
                if (!firstPage) writer.newPage();
                firstPage = false;
                painter.fillRect(QRect(0, 0, writer.width(), writer.height()), Qt::white);
                painter.setPen(QColor("#202329"));
                QFont titleFont(QStringLiteral("Noto Sans"), 14, QFont::DemiBold);
                painter.setFont(titleFont);
                painter.drawText(margin, 40, displayName());
                painter.setFont(QFont(QStringLiteral("Noto Sans"), 8));
                const int columnStart = horizontal * 8;
                const int columnEnd = qMin(lastColumn, columnStart + 7);
                for (int column = columnStart; column <= columnEnd; ++column) {
                    const QRect box(margin + (column - columnStart) * cellWidth, 65, cellWidth, rowHeight);
                    painter.fillRect(box, QColor("#eef1f5"));
                    painter.drawRect(box);
                    painter.drawText(box.adjusted(5, 0, -5, 0), Qt::AlignVCenter, columnName(column));
                }
                for (int offset = 0; offset < 28; ++offset) {
                    const int viewIndex = vertical * 28 + offset;
                    if (viewIndex >= usedRows.size()) break;
                    const int row = usedRows.at(viewIndex);
                    for (int column = columnStart; column <= columnEnd; ++column) {
                        const QRect box(margin + (column - columnStart) * cellWidth,
                                        87 + offset * rowHeight, cellWidth, rowHeight);
                        const QString fill = fillColorAt(row, column);
                        if (!fill.isEmpty()) painter.fillRect(box, QColor(fill));
                        painter.setPen(QColor("#cfd4dc"));
                        painter.drawRect(box);
                        const QString textColor = textColorAt(row, column);
                        painter.setPen(textColor.isEmpty() ? QColor("#202329") : QColor(textColor));
                        QFont font(QStringLiteral("Noto Sans"), 8);
                        font.setBold(boldAt(row, column));
                        painter.setFont(font);
                        const QString value = painter.fontMetrics().elidedText(displayAt(row, column),
                                                                                Qt::ElideRight, cellWidth - 10);
                        const int align = alignmentAt(row, column);
                        painter.drawText(box.adjusted(5, 0, -5, 0),
                                         Qt::AlignVCenter | (align == 1 ? Qt::AlignHCenter :
                                                            align == 2 ? Qt::AlignRight : Qt::AlignLeft), value);
                    }
                }
            }
        }
        const QVariantList points = chartData();
        if (!points.isEmpty()) {
            writer.newPage();
            painter.fillRect(QRect(0, 0, writer.width(), writer.height()), Qt::white);
            painter.setPen(QColor("#202329"));
            painter.setFont(QFont(QStringLiteral("Noto Sans"), 16, QFont::DemiBold));
            painter.drawText(margin, 55, QStringLiteral("Chart · ") + m_chartRange);
            const QRect plot(95, 100, writer.width() - 165, writer.height() - 205);
            if(m_chartType==QStringLiteral("pie")){
                double total=0;for(const QVariant &point:points)total+=qMax(0.0,point.toMap().value("value").toDouble());
                if(total>0){
                    const QList<QColor> colors={QColor("#638be0"),QColor("#e47b89"),QColor("#71bc9a"),QColor("#d6a15e"),
                                                QColor("#ae91df"),QColor("#67b9c5"),QColor("#d980bc"),QColor("#a5b668")};
                    QRectF pie(plot.left()+15,plot.top()+35,qMin(250,plot.height()-65),qMin(250,plot.height()-65));
                    int angle=90*16,colorIndex=0;
                    for(const QVariant &point:points){
                        const QVariantMap item=point.toMap();double value=item.value("value").toDouble();if(value<=0)continue;
                        const int sweep=qRound(value/total*360*16);
                        painter.setPen(Qt::NoPen);painter.setBrush(colors[colorIndex%colors.size()]);
                        painter.drawPie(pie,angle,-sweep);angle-=sweep;
                        if(colorIndex<12){
                            const int legendY=plot.top()+38+colorIndex*24;
                            painter.drawRect(QRectF(pie.right()+30,legendY,10,10));
                            painter.setPen(QColor("#202329"));painter.setFont(QFont(QStringLiteral("Noto Sans"),8));
                            painter.drawText(QRectF(pie.right()+47,legendY-3,plot.right()-pie.right()-53,19),
                                             Qt::AlignVCenter,item.value("label").toString()+"  "+QString::number(qRound(value/total*100))+"%");
                        }
                        ++colorIndex;
                    }
                }
            }else{
            painter.setPen(QPen(QColor("#a9b3c1"), 1));
            painter.drawLine(plot.bottomLeft(), plot.topLeft());
            painter.drawLine(plot.bottomLeft(), plot.bottomRight());
            double maximum = 0;
            double minimum = 0;
            for (const QVariant &point : points) {
                const double value = point.toMap().value(QStringLiteral("value")).toDouble();
                maximum = qMax(maximum, value);
                minimum = qMin(minimum, value);
            }
            const double span = qMax(1.0, maximum - minimum);
            const qreal zeroY = plot.top() + maximum / span * plot.height();
            painter.drawLine(QPointF(plot.left(), zeroY), QPointF(plot.right(), zeroY));
            const qreal slot = static_cast<qreal>(plot.width()) / points.size();
            if(m_chartType==QStringLiteral("area")){
                QPainterPath area;area.moveTo(plot.left()+slot*0.5,zeroY);
                for(int index=0;index<points.size();++index){
                    const double value=points.at(index).toMap().value("value").toDouble();
                    area.lineTo(plot.left()+slot*(index+0.5),plot.top()+(maximum-value)/span*plot.height());
                }
                area.lineTo(plot.left()+slot*(points.size()-0.5),zeroY);area.closeSubpath();
                painter.fillPath(area,QColor(99,139,224,65));
            }
            QPointF previous;
            for (int index = 0; index < points.size(); ++index) {
                const QVariantMap point = points.at(index).toMap();
                const qreal value = point.value(QStringLiteral("value")).toDouble();
                const qreal valueY = plot.top() + (maximum - value) / span * plot.height();
                const qreal center = plot.left() + slot * (index + 0.5);
                painter.setPen(QColor("#4069b7"));
                painter.setBrush(QColor("#638be0"));
                if (m_chartType == QStringLiteral("line") || m_chartType == QStringLiteral("area")) {
                    const QPointF current(center, valueY);
                    if (index > 0) painter.drawLine(previous, current);
                    painter.drawEllipse(current, 3, 3);
                    previous = current;
                } else {
                    painter.drawRect(QRectF(center - slot * 0.31, qMin(valueY, zeroY),
                                           slot * 0.62, qAbs(valueY - zeroY)));
                }
                painter.setPen(QColor("#202329"));
                painter.setFont(QFont(QStringLiteral("Noto Sans"), 8));
                const QRectF label(center - slot / 2, plot.bottom() + 8, slot, 30);
                painter.drawText(label, Qt::AlignHCenter, point.value(QStringLiteral("label")).toString());
            }
            }
        }
        painter.end();
    }
    if (!file.commit()) { setError(QStringLiteral("Could not finish the PDF.")); return false; }
    setError({});
    return true;
}
