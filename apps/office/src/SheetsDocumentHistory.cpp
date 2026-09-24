#include "SheetsDocument.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QSaveFile>
#include <QStandardPaths>
#include <QUuid>

SheetsDocument::HistoryState SheetsDocument::capture() const {
    return {allSheets(), m_activeSheet};
}

void SheetsDocument::restore(const HistoryState &state) {
    beginResetModel();
    m_sheets = state.sheets;
    m_activeSheet = state.active;
    loadActiveState(m_sheets[m_activeSheet]);
    endResetModel();
    m_dirty = QJsonDocument(nativeObject()).toJson(QJsonDocument::Compact) != m_savedState;
    if (!m_dirty) removeSessionRecovery();
    else if (!m_recoveryTimer.isActive()) m_recoveryTimer.start();
    refresh();
    emit dimensionsChanged();
    emit sheetsChanged();
    emit historyChanged();
}

void SheetsDocument::beginDimensionResize() {
    m_dimensionResizeActive = true;
    m_dimensionResizeRecorded = false;
}
void SheetsDocument::endDimensionResize() {
    m_dimensionResizeActive = false;
    m_dimensionResizeRecorded = false;
}
void SheetsDocument::recordEdit() {
    if (m_dimensionResizeActive && m_dimensionResizeRecorded) return;
    if (m_dimensionResizeActive) m_dimensionResizeRecorded = true;
    m_undo.append(capture());
    if (m_undo.size() > 50) m_undo.removeFirst();
    m_redo.clear();
    if (!m_recoveryTimer.isActive()) m_recoveryTimer.start();
    emit historyChanged();
}

void SheetsDocument::clearHistory() {
    m_undo.clear();
    m_redo.clear();
    m_recoveryTimer.stop();
    endDimensionResize();
    emit historyChanged();
}

void SheetsDocument::undo() {
    if (m_undo.isEmpty()) return;
    m_redo.append(capture());
    restore(m_undo.takeLast());
}
void SheetsDocument::redo() {
    if (m_redo.isEmpty()) return;
    m_undo.append(capture());
    restore(m_redo.takeLast());
}

QString SheetsDocument::recoveryDirectory() const {
    return QStandardPaths::writableLocation(QStandardPaths::AppDataLocation) + QStringLiteral("/recovery/sheets");
}
void SheetsDocument::findRecovery() {
    const QFileInfoList files = QDir(recoveryDirectory()).entryInfoList({QStringLiteral("*.json")}, QDir::Files, QDir::Time);
    const QString path = files.isEmpty() ? QString() : files.first().absoluteFilePath();
    if (m_availableRecovery == path) return;
    m_availableRecovery = path;
    emit recoveryAvailableChanged();
}
void SheetsDocument::removeSessionRecovery() { m_recoveryTimer.stop(); QFile::remove(m_sessionRecovery); }
void SheetsDocument::writeRecoveryNow() {
    if (!m_dirty || !m_hasDocument || !QDir().mkpath(recoveryDirectory())) return;
    QJsonObject object = nativeObject();
    object.insert(QStringLiteral("recoveryPath"), m_path);
    const QByteArray data = QJsonDocument(object).toJson(QJsonDocument::Compact);
    QSaveFile file(m_sessionRecovery);
    if (data.size() <= 15 * 1024 * 1024 && file.open(QIODevice::WriteOnly)) {
        if (file.write(data) == data.size()) file.commit();
    }
}
bool SheetsDocument::restoreRecovery() {
    if (m_availableRecovery.isEmpty()) return false;
    const QString recoveryFile = m_availableRecovery;
    QFile file(recoveryFile);
    if (!file.open(QIODevice::ReadOnly)) return false;
    const QJsonObject object = QJsonDocument::fromJson(file.readAll()).object();
    if (!object.value(QStringLiteral("recoveryPath")).isString() || !openNative(recoveryFile)) return false;
    m_path = object.value(QStringLiteral("recoveryPath")).toString();
    m_dirty = true;
    m_savedState.clear();
    m_availableRecovery.clear();
    QFile::remove(recoveryFile);
    writeRecoveryNow();
    m_recoveryTimer.start();
    emit recoveryAvailableChanged();
    emit stateChanged();
    return true;
}
void SheetsDocument::discardRecovery() {
    removeSessionRecovery();
    if (!m_availableRecovery.isEmpty()) {
        QFile::remove(m_availableRecovery);
        m_availableRecovery.clear();
        emit recoveryAvailableChanged();
    }
}
void SheetsDocument::discardSessionRecovery() { removeSessionRecovery(); }
