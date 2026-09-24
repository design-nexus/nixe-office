#include "SheetsDocument.h"
#include <QClipboard>
#include <QGuiApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMimeData>
#include <QRegularExpression>
#include <algorithm>

namespace {
QString shiftReferences(const QString &value, int rowDelta, int columnDelta) {
    if (!value.startsWith('=')) return value;
    static const QRegularExpression reference("(?<![A-Za-z0-9_])([A-Za-z]{1,2})([1-9][0-9]*)");
    QString result;
    int previous = 0;
    auto matches = reference.globalMatch(value);
    while (matches.hasNext()) {
        const auto match = matches.next();
        result += value.mid(previous, match.capturedStart() - previous);
        int column = 0;
        for (QChar letter : match.captured(1)) column = column * 26 + letter.toUpper().unicode() - 'A' + 1;
        const int newColumn = column - 1 + columnDelta;
        const int newRow = match.captured(2).toInt() - 1 + rowDelta;
        if (newColumn < 0 || newColumn >= 52 || newRow < 0 || newRow >= 500) result += "#REF!";
        else {
            QString name;
            for (int c = newColumn + 1; c; c = (c - 1) / 26) name.prepend(QChar('A' + (c - 1) % 26));
            result += name + QString::number(newRow + 1);
        }
        previous = match.capturedEnd();
    }
    return result + value.mid(previous);
}
}

SheetsDocument::SheetState SheetsDocument::activeState() const {
    SheetState s;
    if (m_activeSheet >= 0 && m_activeSheet < m_sheets.size()) s.name=m_sheets[m_activeSheet].name;
    s.cells=m_cells;s.numberFormats=m_numberFormats;s.alignments=m_alignments;
    s.fillColors=m_fillColors;s.textColors=m_textColors;s.boldCells=m_boldCells;
    s.rowHeights=m_rowHeights;s.columnWidths=m_columnWidths;s.viewRows=m_viewRows;
    s.sortColumn=m_sortColumn;s.filterColumn=m_filterColumn;s.sortAscending=m_sortAscending;
    s.filterQuery=m_filterQuery;s.chartRange=m_chartRange;s.chartType=m_chartType;
    return s;
}
void SheetsDocument::loadActiveState(const SheetState &s) {
    m_cells=s.cells;m_numberFormats=s.numberFormats;m_alignments=s.alignments;
    m_fillColors=s.fillColors;m_textColors=s.textColors;m_boldCells=s.boldCells;
    m_rowHeights=s.rowHeights;m_columnWidths=s.columnWidths;m_viewRows=s.viewRows;
    m_sortColumn=s.sortColumn;m_filterColumn=s.filterColumn;m_sortAscending=s.sortAscending;
    m_filterQuery=s.filterQuery;m_chartRange=s.chartRange;m_chartType=s.chartType;
    if(m_viewRows.isEmpty())for(int row=0;row<rows;++row)m_viewRows.append(row);
}
QVector<SheetsDocument::SheetState> SheetsDocument::allSheets() const {
    QVector<SheetState> result=m_sheets;
    if(result.isEmpty())result.append(SheetState());
    if(m_activeSheet>=0&&m_activeSheet<result.size())result[m_activeSheet]=activeState();
    return result;
}
QStringList SheetsDocument::sheetNames() const {QStringList names;for(const auto &s:m_sheets)names.append(s.name);return names;}
void SheetsDocument::selectSheet(int index){
    if(index<0||index>=m_sheets.size()||index==m_activeSheet)return;
    beginResetModel();m_sheets[m_activeSheet]=activeState();m_activeSheet=index;loadActiveState(m_sheets[index]);endResetModel();
    if(m_sortColumn>=0||!m_filterQuery.isEmpty())rebuildView();
    refresh();emit dimensionsChanged();emit sheetsChanged();
}
void SheetsDocument::addSheet(){
    if(m_sheets.size()>=20){setError("A workbook can contain up to 20 sheets.");return;}
    recordEdit();m_sheets[m_activeSheet]=activeState();
    QSet<QString> existing;for(const auto &s:m_sheets)existing.insert(s.name.toCaseFolded());
    int n=1;while(existing.contains(QString("Sheet %1").arg(n).toCaseFolded()))++n;
    SheetState s;s.name=QString("Sheet %1").arg(n);for(int row=0;row<rows;++row)s.viewRows.append(row);
    beginResetModel();m_sheets.append(s);m_activeSheet=m_sheets.size()-1;loadActiveState(s);endResetModel();
    m_dirty=true;m_hasDocument=true;setError({});refresh();emit dimensionsChanged();emit sheetsChanged();
}
void SheetsDocument::duplicateSheet(){
    if(m_sheets.size()>=20){setError("A workbook can contain up to 20 sheets.");return;}
    recordEdit();m_sheets[m_activeSheet]=activeState();SheetState s=activeState();
    QString base=s.name+" copy",name=base;int n=2;
    auto taken=[&](const QString &candidate){for(const auto &sheet:m_sheets)if(sheet.name.compare(candidate,Qt::CaseInsensitive)==0)return true;return false;};
    while(taken(name))name=base+" "+QString::number(n++);s.name=name;
    beginResetModel();m_sheets.insert(m_activeSheet+1,s);++m_activeSheet;loadActiveState(s);endResetModel();
    m_dirty=true;refresh();emit dimensionsChanged();emit sheetsChanged();
}
void SheetsDocument::renameSheet(int index,const QString &name){
    QString clean=name.trimmed();if(index<0||index>=m_sheets.size()||clean.isEmpty()||clean.size()>31||clean.contains(QRegularExpression("[\\[\\]\\*\\?/\\\\:]") )){setError("Use a unique sheet name up to 31 characters without : \\ / ? * [ ].");return;}
    for(int i=0;i<m_sheets.size();++i)if(i!=index&&m_sheets[i].name.compare(clean,Qt::CaseInsensitive)==0){setError("That sheet name is already in use.");return;}
    if(m_sheets[index].name==clean)return;recordEdit();m_sheets[index].name=clean;m_dirty=true;setError({});emit sheetsChanged();emit stateChanged();
}
void SheetsDocument::removeSheet(int index){
    if(index<0||index>=m_sheets.size()||m_sheets.size()==1)return;recordEdit();m_sheets[m_activeSheet]=activeState();
    beginResetModel();m_sheets.removeAt(index);if(index<m_activeSheet)--m_activeSheet;else if(index==m_activeSheet)m_activeSheet=std::min(m_activeSheet,int(m_sheets.size())-1);loadActiveState(m_sheets[m_activeSheet]);endResetModel();
    if(m_sortColumn>=0||!m_filterQuery.isEmpty())rebuildView();
    m_dirty=true;refresh();emit dimensionsChanged();emit sheetsChanged();
}
void SheetsDocument::copyRange(int firstRow,int firstColumn,int lastRow,int lastColumn){
    if(!QGuiApplication::clipboard())return;
    int top=std::clamp(std::min(firstRow,lastRow),0,rows-1),bottom=std::clamp(std::max(firstRow,lastRow),0,rows-1);
    int left=std::clamp(std::min(firstColumn,lastColumn),0,columns-1),right=std::clamp(std::max(firstColumn,lastColumn),0,columns-1);
    QStringList lines;for(int row=top;row<=bottom;++row){QStringList fields;for(int col=left;col<=right;++col){QString value=rawAt(row,col);value.replace('\t',' ').replace('\n',' ');fields.append(value);}lines.append(fields.join('\t'));}
    QJsonObject payload{{"row",top},{"column",left},{"text",lines.join('\n')}};
    auto *mime=new QMimeData;
    mime->setText(lines.join('\n'));
    mime->setData("application/x-nexus-sheets-range",QJsonDocument(payload).toJson(QJsonDocument::Compact));
    QGuiApplication::clipboard()->setMimeData(mime);
}
void SheetsDocument::pasteRange(int row,int column){
    if(row<0||row>=rows||column<0||column>=columns||!QGuiApplication::clipboard())return;
    QString text=QGuiApplication::clipboard()->text();if(text.isEmpty()||text.size()>5*1024*1024)return;
    const QMimeData *mime=QGuiApplication::clipboard()->mimeData();
    QJsonObject source=mime&&mime->hasFormat("application/x-nexus-sheets-range")?
        QJsonDocument::fromJson(mime->data("application/x-nexus-sheets-range")).object():QJsonObject();
    bool shiftFormulas=source.value("text").toString()==text&&source.value("row").isDouble()&&source.value("column").isDouble();
    int rowDelta=row-source.value("row").toInt(),columnDelta=column-source.value("column").toInt();
    auto shifted=[&](const QString &value){ return shiftFormulas ? shiftReferences(value,rowDelta,columnDelta) : value; };
    QStringList lines=text.split('\n');if(!lines.isEmpty()&&lines.last().isEmpty())lines.removeLast();
    QHash<int,QString> edits;for(int r=0;r<lines.size()&&row+r<rows;++r){QStringList fields=lines[r].remove(QRegularExpression("\\r$")).split('\t');for(int c=0;c<fields.size()&&column+c<columns;++c)edits.insert(key(row+r,column+c),shifted(fields[c]));}
    if(edits.isEmpty())return;recordEdit();for(auto it=edits.cbegin();it!=edits.cend();++it){if(it.value().isEmpty())m_cells.remove(it.key());else m_cells.insert(it.key(),it.value());}
    m_dirty=true;m_hasDocument=true;if(m_sortColumn>=0||!m_filterQuery.isEmpty())rebuildView();refresh();
}

void SheetsDocument::pasteRangeToSelection(int firstRow, int firstColumn, int lastRow, int lastColumn) {
    const int top = std::min(firstRow, lastRow), bottom = std::max(firstRow, lastRow);
    const int left = std::min(firstColumn, lastColumn), right = std::max(firstColumn, lastColumn);
    if (top < 0 || bottom >= rows || left < 0 || right >= columns || !QGuiApplication::clipboard()) return;
    if (top == bottom && left == right) { pasteRange(top, left); return; }
    QString text = QGuiApplication::clipboard()->text();
    if (text.isEmpty() || text.size() > 5 * 1024 * 1024) return;
    QStringList lines = text.split('\n');
    if (!lines.isEmpty() && lines.last().isEmpty()) lines.removeLast();
    QVector<QStringList> fields;
    int sourceWidth = 0;
    for (QString line : lines) {
        line.remove(QRegularExpression("\\r$"));
        fields.append(line.split('\t'));
        sourceWidth = std::max(sourceWidth, int(fields.last().size()));
    }
    if (fields.isEmpty() || sourceWidth == 0) return;
    if (fields.size() > bottom - top + 1 || sourceWidth > right - left + 1) {
        pasteRange(top, left);
        return;
    }
    const QMimeData *mime = QGuiApplication::clipboard()->mimeData();
    const QJsonObject source = mime && mime->hasFormat("application/x-nexus-sheets-range")
        ? QJsonDocument::fromJson(mime->data("application/x-nexus-sheets-range")).object() : QJsonObject();
    const bool shiftFormulas = source.value("text").toString() == text &&
        source.value("row").isDouble() && source.value("column").isDouble();
    recordEdit();
    for (int row = top; row <= bottom; ++row) for (int column = left; column <= right; ++column) {
        const int sourceRowOffset = (row - top) % fields.size();
        const int sourceColumnOffset = (column - left) % sourceWidth;
        const QStringList &sourceLine = fields[sourceRowOffset];
        QString value = sourceColumnOffset < sourceLine.size() ? sourceLine[sourceColumnOffset] : QString();
        if (shiftFormulas)
            value = shiftReferences(value, row - source.value("row").toInt() - sourceRowOffset,
                                    column - source.value("column").toInt() - sourceColumnOffset);
        const int destination = key(row, column);
        if (value.isEmpty()) m_cells.remove(destination); else m_cells.insert(destination, value);
    }
    m_dirty = true; m_hasDocument = true;
    if (m_sortColumn >= 0 || !m_filterQuery.isEmpty()) rebuildView();
    refresh();
}

void SheetsDocument::clearRange(int firstRow, int firstColumn, int lastRow, int lastColumn) {
    const int top = std::min(firstRow, lastRow), bottom = std::max(firstRow, lastRow);
    const int left = std::min(firstColumn, lastColumn), right = std::max(firstColumn, lastColumn);
    if (top < 0 || bottom >= rows || left < 0 || right >= columns) return;
    bool changed = false;
    for (int row = top; row <= bottom && !changed; ++row)
        for (int column = left; column <= right; ++column)
            if (m_cells.contains(key(row, column))) { changed = true; break; }
    if (!changed) return;
    recordEdit();
    for (int row = top; row <= bottom; ++row)
        for (int column = left; column <= right; ++column) m_cells.remove(key(row, column));
    m_dirty = true; m_hasDocument = true;
    if (m_sortColumn >= 0 || !m_filterQuery.isEmpty()) rebuildView();
    refresh();
}

void SheetsDocument::fillRange(int firstRow, int firstColumn, int lastRow, int lastColumn,
                               int targetRow, int targetColumn) {
    const int top = std::min(firstRow, lastRow), bottom = std::max(firstRow, lastRow);
    const int left = std::min(firstColumn, lastColumn), right = std::max(firstColumn, lastColumn);
    if (top < 0 || bottom >= rows || left < 0 || right >= columns ||
        targetRow < 0 || targetRow >= rows || targetColumn < 0 || targetColumn >= columns) return;
    const int verticalDistance = targetRow < top ? top - targetRow : std::max(0, targetRow - bottom);
    const int horizontalDistance = targetColumn < left ? left - targetColumn : std::max(0, targetColumn - right);
    if (!verticalDistance && !horizontalDistance) return;
    const bool vertical = verticalDistance >= horizontalDistance;
    const int fillTop = vertical ? std::min(top, targetRow) : top;
    const int fillBottom = vertical ? std::max(bottom, targetRow) : bottom;
    const int fillLeft = vertical ? left : std::min(left, targetColumn);
    const int fillRight = vertical ? right : std::max(right, targetColumn);
    const auto values = m_cells;
    const auto formats = m_numberFormats;
    const auto alignments = m_alignments;
    const auto fills = m_fillColors;
    const auto textColors = m_textColors;
    const auto bold = m_boldCells;
    const int sourceHeight = bottom - top + 1, sourceWidth = right - left + 1;
    auto wrap = [](int value, int size) { return (value % size + size) % size; };
    recordEdit();
    for (int row = fillTop; row <= fillBottom; ++row) for (int column = fillLeft; column <= fillRight; ++column) {
        if (row >= top && row <= bottom && column >= left && column <= right) continue;
        const int sourceRow = top + wrap(row - top, sourceHeight);
        const int sourceColumn = left + wrap(column - left, sourceWidth);
        const int source = key(sourceRow, sourceColumn), destination = key(row, column);
        const QString value = shiftReferences(values.value(source), row - sourceRow, column - sourceColumn);
        if (value.isEmpty()) m_cells.remove(destination); else m_cells.insert(destination, value);
        if (formats.contains(source)) m_numberFormats.insert(destination, formats.value(source));
        else m_numberFormats.remove(destination);
        if (alignments.contains(source)) m_alignments.insert(destination, alignments.value(source));
        else m_alignments.remove(destination);
        if (fills.contains(source)) m_fillColors.insert(destination, fills.value(source));
        else m_fillColors.remove(destination);
        if (textColors.contains(source)) m_textColors.insert(destination, textColors.value(source));
        else m_textColors.remove(destination);
        if (bold.contains(source)) m_boldCells.insert(destination); else m_boldCells.remove(destination);
    }
    m_dirty = true; m_hasDocument = true;
    if (m_sortColumn >= 0 || !m_filterQuery.isEmpty()) rebuildView();
    refresh();
}

void SheetsDocument::moveRange(int firstRow, int firstColumn, int lastRow, int lastColumn,
                               int targetRow, int targetColumn) {
    const int top = std::min(firstRow, lastRow), bottom = std::max(firstRow, lastRow);
    const int left = std::min(firstColumn, lastColumn), right = std::max(firstColumn, lastColumn);
    const int height = bottom - top + 1, width = right - left + 1;
    if (top < 0 || bottom >= rows || left < 0 || right >= columns || targetRow < 0 || targetColumn < 0 ||
        targetRow + height > rows || targetColumn + width > columns ||
        (targetRow == top && targetColumn == left)) return;
    const auto values = m_cells;
    const auto formats = m_numberFormats;
    const auto alignments = m_alignments;
    const auto fills = m_fillColors;
    const auto textColors = m_textColors;
    const auto bold = m_boldCells;
    recordEdit();
    for (int row = top; row <= bottom; ++row) for (int column = left; column <= right; ++column) {
        const int source = key(row, column);
        m_cells.remove(source); m_numberFormats.remove(source); m_alignments.remove(source);
        m_fillColors.remove(source); m_textColors.remove(source); m_boldCells.remove(source);
    }
    for (int row = 0; row < height; ++row) for (int column = 0; column < width; ++column) {
        const int source = key(top + row, left + column);
        const int destination = key(targetRow + row, targetColumn + column);
        if (values.contains(source)) m_cells.insert(destination, values.value(source));
        else m_cells.remove(destination);
        if (formats.contains(source)) m_numberFormats.insert(destination, formats.value(source));
        else m_numberFormats.remove(destination);
        if (alignments.contains(source)) m_alignments.insert(destination, alignments.value(source));
        else m_alignments.remove(destination);
        if (fills.contains(source)) m_fillColors.insert(destination, fills.value(source));
        else m_fillColors.remove(destination);
        if (textColors.contains(source)) m_textColors.insert(destination, textColors.value(source));
        else m_textColors.remove(destination);
        if (bold.contains(source)) m_boldCells.insert(destination); else m_boldCells.remove(destination);
    }
    m_dirty = true; m_hasDocument = true;
    if (m_sortColumn >= 0 || !m_filterQuery.isEmpty()) rebuildView();
    refresh();
}
