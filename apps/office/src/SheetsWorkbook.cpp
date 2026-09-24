#include "SheetsDocument.h"
#include <QClipboard>
#include <QGuiApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMimeData>
#include <QRegularExpression>
#include <algorithm>

SheetsDocument::SheetState SheetsDocument::activeState() const {
    SheetState s;
    if (m_activeSheet >= 0 && m_activeSheet < m_sheets.size()) s.name=m_sheets[m_activeSheet].name;
    s.cells=m_cells;s.numberFormats=m_numberFormats;s.boldCells=m_boldCells;
    s.rowHeights=m_rowHeights;s.columnWidths=m_columnWidths;s.viewRows=m_viewRows;
    s.sortColumn=m_sortColumn;s.filterColumn=m_filterColumn;s.sortAscending=m_sortAscending;
    s.filterQuery=m_filterQuery;s.chartRange=m_chartRange;s.chartType=m_chartType;
    return s;
}
void SheetsDocument::loadActiveState(const SheetState &s) {
    m_cells=s.cells;m_numberFormats=s.numberFormats;m_boldCells=s.boldCells;
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
    static const QRegularExpression reference("(?<![A-Za-z0-9_])([A-Za-z]{1,2})([1-9][0-9]*)");
    auto shifted=[&](QString value){
        if(!shiftFormulas||!value.startsWith('='))return value;
        QString result;int previous=0;auto matches=reference.globalMatch(value);
        while(matches.hasNext()){auto match=matches.next();result+=value.mid(previous,match.capturedStart()-previous);
            int col=0;for(QChar letter:match.captured(1))col=col*26+letter.toUpper().unicode()-'A'+1;
            int newCol=col-1+columnDelta,newRow=match.captured(2).toInt()-1+rowDelta;
            if(newCol<0||newCol>=columns||newRow<0||newRow>=rows)result+="#REF!";
            else {QString name;for(int c=newCol+1;c;c=(c-1)/26)name.prepend(QChar('A'+(c-1)%26));result+=name+QString::number(newRow+1);}
            previous=match.capturedEnd();}
        result+=value.mid(previous);return result;
    };
    QStringList lines=text.split('\n');if(!lines.isEmpty()&&lines.last().isEmpty())lines.removeLast();
    QHash<int,QString> edits;for(int r=0;r<lines.size()&&row+r<rows;++r){QStringList fields=lines[r].remove(QRegularExpression("\\r$")).split('\t');for(int c=0;c<fields.size()&&column+c<columns;++c)edits.insert(key(row+r,column+c),shifted(fields[c]));}
    if(edits.isEmpty())return;recordEdit();for(auto it=edits.cbegin();it!=edits.cend();++it){if(it.value().isEmpty())m_cells.remove(it.key());else m_cells.insert(it.key(),it.value());}
    m_dirty=true;m_hasDocument=true;if(m_sortColumn>=0||!m_filterQuery.isEmpty())rebuildView();refresh();
}
