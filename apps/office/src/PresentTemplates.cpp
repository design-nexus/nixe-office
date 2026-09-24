#include "PresentDocument.h"

bool PresentDocument::createTemplate(const QString &templateId) {
    if (templateId != QStringLiteral("trip-plan") &&
        templateId != QStringLiteral("celebration") &&
        templateId != QStringLiteral("story")) return false;
    const auto makeSlide = [](const QString &title, const QString &body,
                              const QString &notes, int layout,
                              const QString &background, const QString &foreground) {
        Slide slide;
        slide.title = title;
        slide.body = body;
        slide.notes = notes;
        slide.layout = layout;
        slide.style = background == QStringLiteral("#ffffff") ? 0 : 1;
        slide.backgroundColor = background;
        slide.fontColor = foreground;
        return slide;
    };
    QVector<Slide> slides;
    if (templateId == QStringLiteral("trip-plan")) {
        slides = {
            makeSlide(QStringLiteral("[Our trip]"), QStringLiteral("[Destination] · [Dates]"),
                      QStringLiteral("Introduce the plan and who is traveling."), 0, QStringLiteral("#23364d"), QStringLiteral("#ffffff")),
            makeSlide(QStringLiteral("The route"), QStringLiteral("• [First stop]\n• [Next stop]\n• [Final stop]"),
                      QStringLiteral("Add travel details here."), 1, QStringLiteral("#ffffff"), QStringLiteral("#23364d")),
            makeSlide(QStringLiteral("What we want to do"), QStringLiteral("• [Activity]\n• [Place to visit]\n• [Food to try]"),
                      QStringLiteral("Add dates, tickets, or links in your own notes."), 1, QStringLiteral("#ffffff"), QStringLiteral("#23364d")),
            makeSlide(QStringLiteral("Before we go"), QStringLiteral("• [Booking]\n• [Packing]\n• [Something to remember]"),
                      QStringLiteral("Review bookings and travel documents."), 1, QStringLiteral("#eaf3f5"), QStringLiteral("#23364d"))
        };
    } else if (templateId == QStringLiteral("celebration")) {
        slides = {
            makeSlide(QStringLiteral("[Let’s celebrate!]"), QStringLiteral("[Occasion] · [Date]"),
                      QStringLiteral("Welcome everyone and introduce the occasion."), 0, QStringLiteral("#55395a"), QStringLiteral("#ffffff")),
            makeSlide(QStringLiteral("The plan"), QStringLiteral("• [Time and place]\n• [Food and drinks]\n• [Activities]"),
                      QStringLiteral("Add practical details for guests."), 1, QStringLiteral("#fff8f3"), QStringLiteral("#55395a")),
            makeSlide(QStringLiteral("People and moments"), QStringLiteral("• [Who to thank]\n• [Favorite memory]\n• [A toast or message]"),
                      QStringLiteral("Personalize this slide for the people being celebrated."), 1, QStringLiteral("#fff8f3"), QStringLiteral("#55395a")),
            makeSlide(QStringLiteral("Thank you!"), QStringLiteral("[A closing message]"),
                      QStringLiteral("Close with a warm message."), 2, QStringLiteral("#55395a"), QStringLiteral("#ffffff"))
        };
    } else {
        slides = {
            makeSlide(QStringLiteral("[Our story]"), QStringLiteral("[A subtitle or date]"),
                      QStringLiteral("Introduce the story and who it is about."), 0, QStringLiteral("#284b4a"), QStringLiteral("#ffffff")),
            makeSlide(QStringLiteral("Where it began"), QStringLiteral("[A short memory or opening chapter]"),
                      QStringLiteral("Add the details you want to tell aloud."), 1, QStringLiteral("#f4f5ef"), QStringLiteral("#284b4a")),
            makeSlide(QStringLiteral("A favorite moment"), QStringLiteral("[What happened and why it matters]"),
                      QStringLiteral("Make this personal with a detail people will remember."), 1, QStringLiteral("#f4f5ef"), QStringLiteral("#284b4a")),
            makeSlide(QStringLiteral("What comes next"), QStringLiteral("[A wish, hope, or next chapter]"),
                      QStringLiteral("End with a thought for the future."), 2, QStringLiteral("#284b4a"), QStringLiteral("#ffffff"))
        };
    }
    newDocument();
    beginResetModel();
    m_slides = std::move(slides);
    endResetModel();
    m_selected = 0;
    clearHistory();
    m_dirty = true;
    m_recoveryTimer.start();
    emit selectionChanged();
    emit currentSlideChanged();
    emit stateChanged();
    return true;
}
