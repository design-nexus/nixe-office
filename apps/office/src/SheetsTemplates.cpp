#include "SheetsDocument.h"

bool SheetsDocument::createTemplate(const QString &templateId) {
    if (templateId != QStringLiteral("monthly-budget") &&
        templateId != QStringLiteral("home-inventory") &&
        templateId != QStringLiteral("trip-budget")) return false;
    newDocument();
    const auto put = [this](int row, int column, const QString &value) {
        if (!value.isEmpty()) m_cells.insert(key(row, column), value);
    };
    if (templateId == QStringLiteral("monthly-budget")) {
        const QStringList headers{QStringLiteral("Category"), QStringLiteral("Planned"),
                                  QStringLiteral("Actual"), QStringLiteral("Difference")};
        for (int column = 0; column < headers.size(); ++column) { put(0, column, headers.at(column)); m_boldCells.insert(key(0, column)); }
        const QStringList categories{QStringLiteral("Housing"), QStringLiteral("Utilities"),
                                     QStringLiteral("Groceries"), QStringLiteral("Transport"),
                                     QStringLiteral("Health"), QStringLiteral("Savings"),
                                     QStringLiteral("Fun"), QStringLiteral("Other")};
        for (int i = 0; i < categories.size(); ++i) {
            const int row = i + 1;
            put(row, 0, categories.at(i));
            put(row, 3, QStringLiteral("=B%1-C%1").arg(row + 1));
            for (int column = 1; column <= 3; ++column) m_numberFormats.insert(key(row, column), 2);
        }
        put(9, 0, QStringLiteral("Total"));
        for (int column = 1; column <= 3; ++column) {
            put(9, column, QStringLiteral("=SUM(%1%2:%1%3)").arg(columnName(column)).arg(2).arg(9));
            m_numberFormats.insert(key(9, column), 2);
            m_boldCells.insert(key(9, column));
        }
        m_boldCells.insert(key(9, 0));
        m_columnWidths.insert(0, 180);
        for (int column = 1; column <= 3; ++column) m_columnWidths.insert(column, 132);
    } else if (templateId == QStringLiteral("home-inventory")) {
        const QStringList headers{QStringLiteral("Item"), QStringLiteral("Room"),
                                  QStringLiteral("Quantity"), QStringLiteral("Est. value each"),
                                  QStringLiteral("Total value"), QStringLiteral("Notes")};
        for (int column = 0; column < headers.size(); ++column) { put(0, column, headers.at(column)); m_boldCells.insert(key(0, column)); }
        const QStringList rooms{QStringLiteral("Living room"), QStringLiteral("Kitchen"),
                                QStringLiteral("Bedroom"), QStringLiteral("Garage")};
        for (int i = 0; i < rooms.size(); ++i) {
            const int row = i + 1;
            put(row, 1, rooms.at(i));
            put(row, 4, QStringLiteral("=C%1*D%1").arg(row + 1));
            m_numberFormats.insert(key(row, 3), 2);
            m_numberFormats.insert(key(row, 4), 2);
        }
        put(5, 0, QStringLiteral("Total"));
        put(5, 4, QStringLiteral("=SUM(E2:E5)"));
        m_numberFormats.insert(key(5, 4), 2);
        m_boldCells.insert(key(5, 0)); m_boldCells.insert(key(5, 4));
        m_columnWidths.insert(0, 180); m_columnWidths.insert(1, 140);
        m_columnWidths.insert(3, 145); m_columnWidths.insert(4, 145); m_columnWidths.insert(5, 220);
    } else {
        const QStringList headers{QStringLiteral("Category"), QStringLiteral("Budget"),
                                  QStringLiteral("Spent"), QStringLiteral("Remaining")};
        for (int column = 0; column < headers.size(); ++column) { put(0, column, headers.at(column)); m_boldCells.insert(key(0, column)); }
        const QStringList categories{QStringLiteral("Travel"), QStringLiteral("Stay"),
                                     QStringLiteral("Food"), QStringLiteral("Activities"),
                                     QStringLiteral("Shopping"), QStringLiteral("Other")};
        for (int i = 0; i < categories.size(); ++i) {
            const int row = i + 1;
            put(row, 0, categories.at(i));
            put(row, 3, QStringLiteral("=B%1-C%1").arg(row + 1));
            for (int column = 1; column <= 3; ++column) m_numberFormats.insert(key(row, column), 2);
        }
        put(7, 0, QStringLiteral("Total"));
        for (int column = 1; column <= 3; ++column) {
            put(7, column, QStringLiteral("=SUM(%1%2:%1%3)").arg(columnName(column)).arg(2).arg(7));
            m_numberFormats.insert(key(7, column), 2);
            m_boldCells.insert(key(7, column));
        }
        m_boldCells.insert(key(7, 0));
        m_columnWidths.insert(0, 180);
        for (int column = 1; column <= 3; ++column) m_columnWidths.insert(column, 132);
    }
    clearHistory();
    m_dirty = true;
    m_recoveryTimer.start();
    refresh();
    emit dimensionsChanged();
    return true;
}
