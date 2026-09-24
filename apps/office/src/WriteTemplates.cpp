#include "WriteDocument.h"

#include <QTextDocument>

bool WriteDocument::createTemplate(const QString &templateId) {
    QString html;
    if (templateId == QStringLiteral("letter")) {
        html = QStringLiteral(R"(<html><body style="font-family:'Noto Sans'; font-size:11pt; color:#202329">
<p align="right">[Date]</p><p>[Your name]<br>[Street address]<br>[City, state, postal code]</p>
<p>[Recipient name]<br>[Address]</p><p>Dear [Name],</p>
<p>Write your message here. Start with the reason for your letter, then add the details you want to share.</p>
<p>Best wishes,</p><p>[Your name]</p></body></html>)");
    } else if (templateId == QStringLiteral("recipe")) {
        html = QStringLiteral(R"(<html><body style="font-family:'Noto Sans'; font-size:11pt; color:#202329">
<h1>[Recipe name]</h1><p><b>Servings:</b> [number] &nbsp;&nbsp; <b>Prep:</b> [minutes] &nbsp;&nbsp; <b>Cook:</b> [minutes]</p>
<h2>Ingredients</h2><ul><li>[amount] [ingredient]</li><li>[amount] [ingredient]</li><li>[amount] [ingredient]</li></ul>
<h2>Directions</h2><ol><li>[First step]</li><li>[Next step]</li><li>[Final step]</li></ol>
<h2>Notes</h2><p>[Substitutions, storage tips, or family notes]</p></body></html>)");
    } else if (templateId == QStringLiteral("journal")) {
        html = QStringLiteral(R"(<html><body style="font-family:'Noto Sans'; font-size:11pt; color:#202329">
<h1>[Today’s date]</h1><p><i>A place for the moments you want to remember.</i></p>
<h2>Today</h2><p>[What happened today?]</p>
<h2>Highlights</h2><ul><li>[A good moment]</li><li>[Something you learned]</li></ul>
<h2>Tomorrow</h2><p>[One thing you want to do next]</p></body></html>)");
    } else return false;
    newDocument();
    m_document.setHtml(html);
    m_document.setModified(true);
    emit stateChanged();
    return true;
}
