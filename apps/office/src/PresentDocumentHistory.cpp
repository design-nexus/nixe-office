#include "PresentDocument.h"

#include <QDir>
#include <QDateTime>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QSaveFile>
#include <QStandardPaths>

PresentDocument::HistoryState PresentDocument::capture() const {
    QVector<Slide> slides = m_slides;
    slides.detach();
    return {slides, m_selected};
}
void PresentDocument::restore(const HistoryState &state) {
    beginResetModel();
    m_slides = state.slides;
    m_slides.detach();
    endResetModel();
    m_selected = state.selected;
    m_dirty = QJsonDocument(nativeObject()).toJson(QJsonDocument::Compact) != m_savedState;
    if (!m_dirty) removeSessionRecovery();
    else if (!m_recoveryTimer.isActive()) m_recoveryTimer.start();
    emit selectionChanged();
    emit currentSlideChanged();
    emit stateChanged();
    emit historyChanged();
}
void PresentDocument::recordTextEdit(const QString &field) {
    const QString key = QString::number(m_selected) + QLatin1Char(':') + field;
    const qint64 now = QDateTime::currentMSecsSinceEpoch();
    if (m_lastTextEditKey != key || now - m_lastTextEditAt > 1000) recordEdit();
    m_lastTextEditKey = key;
    m_lastTextEditAt = now;
}
void PresentDocument::recordEdit() {
    m_lastTextEditKey.clear();
    m_undo.append(capture());
    if (m_undo.size() > 50) m_undo.removeFirst();
    m_redo.clear();
    if (!m_recoveryTimer.isActive()) m_recoveryTimer.start();
    emit historyChanged();
}
void PresentDocument::clearHistory() {
    m_undo.clear(); m_redo.clear();
    m_recoveryTimer.stop();
    m_lastTextEditKey.clear();
    emit historyChanged();
}
void PresentDocument::undo() {
    if (m_undo.isEmpty()) return;
    m_lastTextEditKey.clear();
    m_redo.append(capture());
    restore(m_undo.takeLast());
}
void PresentDocument::redo() {
    if (m_redo.isEmpty()) return;
    m_lastTextEditKey.clear();
    m_undo.append(capture());
    restore(m_redo.takeLast());
}
QString PresentDocument::recoveryDirectory() const {
    return QStandardPaths::writableLocation(QStandardPaths::AppDataLocation) + QStringLiteral("/recovery/present");
}
void PresentDocument::findRecovery() {
    const QFileInfoList files = QDir(recoveryDirectory()).entryInfoList({QStringLiteral("*.json")}, QDir::Files, QDir::Time);
    const QString path = files.isEmpty() ? QString() : files.first().absoluteFilePath();
    if (m_availableRecovery == path) return;
    m_availableRecovery = path;
    emit recoveryAvailableChanged();
}
void PresentDocument::removeSessionRecovery() { m_recoveryTimer.stop(); QFile::remove(m_sessionRecovery); }
void PresentDocument::writeRecoveryNow() {
    if (!m_dirty || !m_hasDocument || !QDir().mkpath(recoveryDirectory())) return;
    QJsonObject object = nativeObject();
    object.insert(QStringLiteral("recoveryPath"), m_path);
    object.insert(QStringLiteral("recoverySelected"), m_selected);
    const QByteArray data = QJsonDocument(object).toJson(QJsonDocument::Compact);
    QSaveFile file(m_sessionRecovery);
    if (data.size() <= 20 * 1024 * 1024 && file.open(QIODevice::WriteOnly)) {
        if (file.write(data) == data.size()) file.commit();
    }
}
bool PresentDocument::restoreRecovery() {
    if (m_availableRecovery.isEmpty()) return false;
    const QString recoveryFile = m_availableRecovery;
    QFile file(recoveryFile);
    if (!file.open(QIODevice::ReadOnly)) return false;
    const QJsonObject object = QJsonDocument::fromJson(file.readAll()).object();
    if (!object.value(QStringLiteral("recoveryPath")).isString() || !open(QUrl::fromLocalFile(recoveryFile))) return false;
    selectSlide(qBound(0, object.value(QStringLiteral("recoverySelected")).toInt(), m_slides.size() - 1));
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
void PresentDocument::discardRecovery() {
    removeSessionRecovery();
    if (!m_availableRecovery.isEmpty()) {
        QFile::remove(m_availableRecovery);
        m_availableRecovery.clear();
        emit recoveryAvailableChanged();
    }
}
void PresentDocument::discardSessionRecovery() { removeSessionRecovery(); }
