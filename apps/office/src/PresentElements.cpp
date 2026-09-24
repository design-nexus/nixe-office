#include "PresentDocument.h"
#include <QBuffer>
#include <QFileInfo>
#include <QImageReader>
#include <QUuid>
#include <algorithm>

QVariantList PresentDocument::elementsFor(const Slide &slide) const {
    QVariantList result;
    for (const Element &element : slide.elements) {
        QVariantMap item{{"id",element.id},{"type",element.type},{"x",element.x},{"y",element.y},
                         {"width",element.width},{"height",element.height},{"text",element.text},
                         {"shape",element.shape},{"fill",element.fill}};
        if(element.type=="image")item.insert("image",QStringLiteral("data:image/png;base64,")+QString::fromLatin1(element.image.toBase64()));
        result.append(item);
    }
    return result;
}
QVariantList PresentDocument::currentElements() const {const Slide *slide=current();return slide?elementsFor(*slide):QVariantList();}

bool PresentDocument::addImage(const QUrl &url) {
    Slide *slide=current();QString path=url.isLocalFile()?url.toLocalFile():QString();
    if(!slide||path.isEmpty()||QFileInfo(path).size()>12*1024*1024||slide->elements.size()>=30){setError("Choose an image under 12 MB; each slide supports up to 30 objects.");return false;}
    QImageReader reader(path);reader.setAutoTransform(true);QSize original=reader.size();
    if(!original.isValid()||original.width()>10000||original.height()>10000){setError("This image is too large or unsupported.");return false;}
    if(original.width()>1600||original.height()>900)reader.setScaledSize(original.scaled(1600,900,Qt::KeepAspectRatio));
    QImage image=reader.read();if(image.isNull()){setError("Could not read this image.");return false;}
    QByteArray png;QBuffer buffer(&png);buffer.open(QIODevice::WriteOnly);
    if(!image.save(&buffer,"PNG")||png.size()>3*1024*1024){setError("This image is too large for a slide.");return false;}
    recordEdit();Element element;element.id=QUuid::createUuid().toString(QUuid::WithoutBraces);
    element.type="image";element.image=std::move(png);
    QSize size=image.size().scaled(460,310,Qt::KeepAspectRatio);
    element.width=std::max(60,size.width());element.height=std::max(40,size.height());
    element.x=(960-element.width)/2;element.y=(540-element.height)/2;
    slide->elements.append(element);setError({});changed();return true;
}
void PresentDocument::addTextBox() {
    Slide *slide=current();if(!slide)return;
    if(slide->elements.size()>=30){setError("A slide can contain up to 30 objects.");return;}
    recordEdit();Element element;element.id=QUuid::createUuid().toString(QUuid::WithoutBraces);
    element.type="text";element.x=250;element.y=220;element.width=460;element.height=110;
    element.text="Text box";slide->elements.append(element);setError({});changed();
}
void PresentDocument::addShape(const QString &shape) {
    Slide *slide=current();if(!slide)return;
    if(shape!="rectangle"&&shape!="ellipse")return;
    if(slide->elements.size()>=30){setError("A slide can contain up to 30 objects.");return;}
    recordEdit();Element element;element.id=QUuid::createUuid().toString(QUuid::WithoutBraces);
    element.type="shape";element.shape=shape;element.fill="#5273cf";
    element.x=330;element.y=180;element.width=300;element.height=180;
    slide->elements.append(element);setError({});changed();
}
void PresentDocument::setElementFill(const QString &id,const QString &color) {
    Slide *slide=current();QColor fill(color);if(!slide||!fill.isValid())return;
    for(int i=0;i<slide->elements.size();++i)if(slide->elements.at(i).id==id&&slide->elements.at(i).type=="shape"){
        const QString normalized=fill.name();if(slide->elements.at(i).fill==normalized)return;
        recordEdit();slide->elements[i].fill=normalized;changed();return;
    }
}
void PresentDocument::alignElement(const QString &id,const QString &alignment) {
    Slide *slide=current();if(!slide)return;
    for(const Element &element:slide->elements)if(element.id==id){
        int x=element.x,y=element.y;
        if(alignment=="left")x=0;else if(alignment=="center")x=(960-element.width)/2;
        else if(alignment=="right")x=960-element.width;
        else if(alignment=="top")y=0;else if(alignment=="middle")y=(540-element.height)/2;
        else if(alignment=="bottom")y=540-element.height;else return;
        setElementRect(id,x,y,element.width,element.height);emit currentSlideChanged();return;
    }
}
void PresentDocument::setElementText(const QString &id,const QString &text) {
    Slide *slide=current();if(!slide||text.size()>10000)return;
    for(int i=0;i<slide->elements.size();++i)if(slide->elements.at(i).id==id&&slide->elements.at(i).type=="text"){
        if(slide->elements.at(i).text==text)return;recordTextEdit("element:"+id);slide->elements[i].text=text;
        m_dirty=true;m_hasDocument=true;emit dataChanged(index(m_selected),index(m_selected));emit stateChanged();return;}
}
void PresentDocument::setElementRect(const QString &id,int x,int y,int width,int height) {
    Slide *slide=current();if(!slide)return;
    for(int i=0;i<slide->elements.size();++i)if(slide->elements.at(i).id==id){
        width=std::clamp(width,48,960);height=std::clamp(height,32,540);
        x=std::clamp(x,0,960-width);y=std::clamp(y,0,540-height);
        const Element &before=slide->elements.at(i);
        if(before.x==x&&before.y==y&&before.width==width&&before.height==height)return;
        recordEdit();Element &element=slide->elements[i];element.x=x;element.y=y;element.width=width;element.height=height;
        m_dirty=true;m_hasDocument=true;emit dataChanged(index(m_selected),index(m_selected));emit stateChanged();return;
    }
}
void PresentDocument::removeElement(const QString &id) {
    Slide *slide=current();if(!slide)return;
    for(int i=0;i<slide->elements.size();++i)if(slide->elements[i].id==id){recordEdit();slide->elements.removeAt(i);changed();return;}
}
