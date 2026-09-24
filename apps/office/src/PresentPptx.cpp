#include "PresentDocument.h"
#include "OfficeZip.h"
#include <QFileInfo>
#include <QJsonDocument>
#include <QXmlStreamWriter>
#include <QBuffer>
#include <QImage>
#include <QImageReader>
#include <QUuid>
#include <algorithm>

namespace {
const QString pns="http://schemas.openxmlformats.org/presentationml/2006/main";
const QString ans="http://schemas.openxmlformats.org/drawingml/2006/main";
const QString rns="http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const QString packageRel="http://schemas.openxmlformats.org/package/2006/relationships";
QByteArray xml(const std::function<void(QXmlStreamWriter&)> &body){QByteArray bytes;QXmlStreamWriter w(&bytes);w.writeStartDocument();body(w);w.writeEndDocument();return bytes;}
void rel(QXmlStreamWriter &w,QString id,QString type,QString target){w.writeStartElement("Relationship");w.writeAttribute("Id",id);w.writeAttribute("Type",rns+"/"+type);w.writeAttribute("Target",target);w.writeEndElement();}
QString hexColor(QString value,QString fallback){if(value.startsWith('#'))value.remove(0,1);return value.size()==6?value.toUpper():fallback;}
void textShape(QXmlStreamWriter &w,int id,const QString &name,const QString &type,const QString &text,qint64 x,qint64 y,qint64 cx,qint64 cy,const QString &font,const QString &color,bool bold,bool italic,int size){
    w.writeStartElement("p:sp");w.writeStartElement("p:nvSpPr");w.writeStartElement("p:cNvPr");w.writeAttribute("id",QString::number(id));w.writeAttribute("name",name);w.writeEndElement();w.writeEmptyElement("p:cNvSpPr");w.writeStartElement("p:nvPr");if(!type.isEmpty()){w.writeStartElement("p:ph");w.writeAttribute("type",type);w.writeEndElement();}w.writeEndElement();w.writeEndElement();
    w.writeStartElement("p:spPr");w.writeStartElement("a:xfrm");w.writeStartElement("a:off");w.writeAttribute("x",QString::number(x));w.writeAttribute("y",QString::number(y));w.writeEndElement();w.writeStartElement("a:ext");w.writeAttribute("cx",QString::number(cx));w.writeAttribute("cy",QString::number(cy));w.writeEndElement();w.writeEndElement();w.writeStartElement("a:prstGeom");w.writeAttribute("prst","rect");w.writeEmptyElement("a:avLst");w.writeEndElement();w.writeEndElement();
    w.writeStartElement("p:txBody");w.writeEmptyElement("a:bodyPr");w.writeEmptyElement("a:lstStyle");QStringList lines=text.split('\n',Qt::KeepEmptyParts);for(const QString &line:lines){w.writeStartElement("a:p");w.writeStartElement("a:r");w.writeStartElement("a:rPr");w.writeAttribute("lang","en-US");w.writeAttribute("sz",QString::number(size*100));if(bold)w.writeAttribute("b","1");if(italic)w.writeAttribute("i","1");w.writeStartElement("a:solidFill");w.writeStartElement("a:srgbClr");w.writeAttribute("val",color);w.writeEndElement();w.writeEndElement();w.writeStartElement("a:latin");w.writeAttribute("typeface",font);w.writeEndElement();w.writeEndElement();w.writeTextElement("a:t",line);w.writeEndElement();w.writeEndElement();}w.writeEndElement();w.writeEndElement();
}
void imageShape(QXmlStreamWriter &w,int id,const QString &relation,qint64 x,qint64 y,qint64 cx,qint64 cy){
    w.writeStartElement("p:pic");w.writeStartElement("p:nvPicPr");
    w.writeStartElement("p:cNvPr");w.writeAttribute("id",QString::number(id));w.writeAttribute("name","Picture "+QString::number(id));w.writeEndElement();
    w.writeStartElement("p:cNvPicPr");w.writeEmptyElement("a:picLocks");w.writeEndElement();w.writeEmptyElement("p:nvPr");w.writeEndElement();
    w.writeStartElement("p:blipFill");w.writeStartElement("a:blip");w.writeAttribute(rns,"embed",relation);w.writeEndElement();
    w.writeStartElement("a:stretch");w.writeEmptyElement("a:fillRect");w.writeEndElement();w.writeEndElement();
    w.writeStartElement("p:spPr");w.writeStartElement("a:xfrm");
    w.writeStartElement("a:off");w.writeAttribute("x",QString::number(x));w.writeAttribute("y",QString::number(y));w.writeEndElement();
    w.writeStartElement("a:ext");w.writeAttribute("cx",QString::number(cx));w.writeAttribute("cy",QString::number(cy));w.writeEndElement();w.writeEndElement();
    w.writeStartElement("a:prstGeom");w.writeAttribute("prst","rect");w.writeEmptyElement("a:avLst");w.writeEndElement();w.writeEndElement();w.writeEndElement();
}
void basicShape(QXmlStreamWriter &w,int id,const QString &shape,const QString &fill,qint64 x,qint64 y,qint64 cx,qint64 cy){
    w.writeStartElement("p:sp");w.writeStartElement("p:nvSpPr");
    w.writeStartElement("p:cNvPr");w.writeAttribute("id",QString::number(id));w.writeAttribute("name","Shape "+QString::number(id));w.writeEndElement();
    w.writeEmptyElement("p:cNvSpPr");w.writeEmptyElement("p:nvPr");w.writeEndElement();
    w.writeStartElement("p:spPr");w.writeStartElement("a:xfrm");
    w.writeStartElement("a:off");w.writeAttribute("x",QString::number(x));w.writeAttribute("y",QString::number(y));w.writeEndElement();
    w.writeStartElement("a:ext");w.writeAttribute("cx",QString::number(cx));w.writeAttribute("cy",QString::number(cy));w.writeEndElement();w.writeEndElement();
    w.writeStartElement("a:prstGeom");w.writeAttribute("prst",shape=="ellipse"?"ellipse":"rect");w.writeEmptyElement("a:avLst");w.writeEndElement();
    w.writeStartElement("a:solidFill");w.writeStartElement("a:srgbClr");w.writeAttribute("val",hexColor(fill,"5273CF"));w.writeEndElement();w.writeEndElement();
    w.writeEndElement();w.writeEndElement();
}
QString relPart(const QString &part){int slash=part.lastIndexOf('/');return part.left(slash+1)+"_rels/"+part.mid(slash+1)+".rels";}
}

bool PresentDocument::exportPptx(const QUrl &url){
    QString path=url.isLocalFile()?url.toLocalFile():QString();if(path.isEmpty()){setError("Choose a local PPTX destination.");return false;}if(!path.endsWith(".pptx",Qt::CaseInsensitive))path+=".pptx";
    QMap<QString,QByteArray> parts;
    parts["[Content_Types].xml"]=xml([&](QXmlStreamWriter &w){w.writeStartElement("Types");w.writeDefaultNamespace("http://schemas.openxmlformats.org/package/2006/content-types");auto def=[&](QString ext,QString ct){w.writeStartElement("Default");w.writeAttribute("Extension",ext);w.writeAttribute("ContentType",ct);w.writeEndElement();};auto over=[&](QString name,QString ct){w.writeStartElement("Override");w.writeAttribute("PartName",name);w.writeAttribute("ContentType",ct);w.writeEndElement();};def("rels","application/vnd.openxmlformats-package.relationships+xml");def("xml","application/xml");def("png","image/png");over("/ppt/presentation.xml","application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml");over("/ppt/slideMasters/slideMaster1.xml","application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml");over("/ppt/slideLayouts/slideLayout1.xml","application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml");over("/ppt/theme/theme1.xml","application/vnd.openxmlformats-officedocument.theme+xml");for(int i=0;i<m_slides.size();++i)over("/ppt/slides/slide"+QString::number(i+1)+".xml","application/vnd.openxmlformats-officedocument.presentationml.slide+xml");w.writeEndElement();});
    parts["_rels/.rels"]=xml([](QXmlStreamWriter &w){w.writeStartElement("Relationships");w.writeDefaultNamespace(packageRel);rel(w,"rId1","officeDocument","ppt/presentation.xml");w.writeEndElement();});
    parts["ppt/presentation.xml"]=xml([&](QXmlStreamWriter &w){w.writeStartElement("p:presentation");w.writeNamespace(pns,"p");w.writeNamespace(rns,"r");w.writeStartElement("p:sldMasterIdLst");w.writeStartElement("p:sldMasterId");w.writeAttribute("id","2147483648");w.writeAttribute(rns,"id","rId1");w.writeEndElement();w.writeEndElement();w.writeStartElement("p:sldIdLst");for(int i=0;i<m_slides.size();++i){w.writeStartElement("p:sldId");w.writeAttribute("id",QString::number(256+i));w.writeAttribute(rns,"id","rId"+QString::number(i+2));w.writeEndElement();}w.writeEndElement();w.writeStartElement("p:sldSz");w.writeAttribute("cx","12192000");w.writeAttribute("cy","6858000");w.writeAttribute("type","screen16x9");w.writeEndElement();w.writeStartElement("p:notesSz");w.writeAttribute("cx","6858000");w.writeAttribute("cy","9144000");w.writeEndElement();w.writeEndElement();});
    parts["ppt/_rels/presentation.xml.rels"]=xml([&](QXmlStreamWriter &w){w.writeStartElement("Relationships");w.writeDefaultNamespace(packageRel);rel(w,"rId1","slideMaster","slideMasters/slideMaster1.xml");for(int i=0;i<m_slides.size();++i)rel(w,"rId"+QString::number(i+2),"slide","slides/slide"+QString::number(i+1)+".xml");w.writeEndElement();});
    parts["ppt/slideMasters/slideMaster1.xml"]=xml([](QXmlStreamWriter &w){w.writeStartElement("p:sldMaster");w.writeNamespace(pns,"p");w.writeNamespace(ans,"a");w.writeNamespace(rns,"r");w.writeStartElement("p:cSld");w.writeStartElement("p:spTree");w.writeStartElement("p:nvGrpSpPr");w.writeStartElement("p:cNvPr");w.writeAttribute("id","1");w.writeAttribute("name","");w.writeEndElement();w.writeEmptyElement("p:cNvGrpSpPr");w.writeEmptyElement("p:nvPr");w.writeEndElement();w.writeEmptyElement("p:grpSpPr");w.writeEndElement();w.writeEndElement();w.writeStartElement("p:clrMap");for(QString k:{"accent1","accent2","accent3","accent4","accent5","accent6"})w.writeAttribute(k,k);w.writeAttribute("bg1","lt1");w.writeAttribute("tx1","dk1");w.writeAttribute("bg2","lt2");w.writeAttribute("tx2","dk2");w.writeAttribute("hlink","hlink");w.writeAttribute("folHlink","folHlink");w.writeEndElement();w.writeStartElement("p:sldLayoutIdLst");w.writeStartElement("p:sldLayoutId");w.writeAttribute("id","2147483649");w.writeAttribute(rns,"id","rId1");w.writeEndElement();w.writeEndElement();w.writeEmptyElement("p:txStyles");w.writeEndElement();});
    parts["ppt/slideMasters/_rels/slideMaster1.xml.rels"]=xml([](QXmlStreamWriter &w){w.writeStartElement("Relationships");w.writeDefaultNamespace(packageRel);rel(w,"rId1","slideLayout","../slideLayouts/slideLayout1.xml");rel(w,"rId2","theme","../theme/theme1.xml");w.writeEndElement();});
    parts["ppt/slideLayouts/slideLayout1.xml"]=xml([](QXmlStreamWriter &w){w.writeStartElement("p:sldLayout");w.writeNamespace(pns,"p");w.writeNamespace(ans,"a");w.writeAttribute("type","blank");w.writeAttribute("preserve","1");w.writeStartElement("p:cSld");w.writeAttribute("name","Nexus Office");w.writeStartElement("p:spTree");w.writeStartElement("p:nvGrpSpPr");w.writeStartElement("p:cNvPr");w.writeAttribute("id","1");w.writeAttribute("name","");w.writeEndElement();w.writeEmptyElement("p:cNvGrpSpPr");w.writeEmptyElement("p:nvPr");w.writeEndElement();w.writeEmptyElement("p:grpSpPr");w.writeEndElement();w.writeEndElement();w.writeEndElement();});
    parts["ppt/slideLayouts/_rels/slideLayout1.xml.rels"]=xml([](QXmlStreamWriter &w){w.writeStartElement("Relationships");w.writeDefaultNamespace(packageRel);rel(w,"rId1","slideMaster","../slideMasters/slideMaster1.xml");w.writeEndElement();});
    parts["ppt/theme/theme1.xml"]=xml([](QXmlStreamWriter &w){w.writeStartElement("a:theme");w.writeNamespace(ans,"a");w.writeAttribute("name","Nexus Office");w.writeStartElement("a:themeElements");w.writeStartElement("a:clrScheme");w.writeAttribute("name","Nexus Office");for(auto pair:QList<QPair<QString,QString>>{{"dk1","202536"},{"lt1","FFFFFF"},{"dk2","343A46"},{"lt2","F4F5F7"},{"accent1","5273CF"},{"accent2","D66A76"},{"accent3","65A888"},{"accent4","B38CD8"},{"accent5","D89A58"},{"accent6","5EA8B7"},{"hlink","5273CF"},{"folHlink","8057A3"}}){w.writeStartElement("a:"+pair.first);w.writeStartElement("a:srgbClr");w.writeAttribute("val",pair.second);w.writeEndElement();w.writeEndElement();}w.writeEndElement();w.writeStartElement("a:fontScheme");w.writeAttribute("name","Nexus Office");for(QString x:{"majorFont","minorFont"}){w.writeStartElement("a:"+x);w.writeStartElement("a:latin");w.writeAttribute("typeface","Noto Sans");w.writeEndElement();w.writeEndElement();}w.writeEndElement();w.writeStartElement("a:fmtScheme");w.writeAttribute("name","Nexus Office");w.writeEmptyElement("a:fillStyleLst");w.writeEmptyElement("a:lnStyleLst");w.writeEmptyElement("a:effectStyleLst");w.writeEmptyElement("a:bgFillStyleLst");w.writeEndElement();w.writeEndElement();w.writeEndElement();});
    int mediaNumber=0;
    for(int i=0;i<m_slides.size();++i){
        const Slide &s=m_slides[i];
        QMap<QString,QString> imageRelations;
        for(const Element &e:s.elements)if(e.type=="image"){
            QString id="rId"+QString::number(imageRelations.size()+2);
            QString name="image"+QString::number(++mediaNumber)+".png";
            imageRelations.insert(e.id,id);
            parts["ppt/media/"+name]=e.image;
        }
        parts["ppt/slides/slide"+QString::number(i+1)+".xml"]=xml([&](QXmlStreamWriter &w){
            w.writeStartElement("p:sld");w.writeNamespace(pns,"p");w.writeNamespace(ans,"a");w.writeNamespace(rns,"r");
            w.writeStartElement("p:cSld");w.writeStartElement("p:bg");w.writeStartElement("p:bgPr");
            w.writeStartElement("a:solidFill");w.writeStartElement("a:srgbClr");
            w.writeAttribute("val",hexColor(s.backgroundColor,s.style==1?"202536":"FFFFFF"));
            w.writeEndElement();w.writeEndElement();w.writeEmptyElement("a:effectLst");w.writeEndElement();w.writeEndElement();
            w.writeStartElement("p:spTree");w.writeStartElement("p:nvGrpSpPr");w.writeStartElement("p:cNvPr");
            w.writeAttribute("id","1");w.writeAttribute("name","");w.writeEndElement();w.writeEmptyElement("p:cNvGrpSpPr");
            w.writeEmptyElement("p:nvPr");w.writeEndElement();w.writeEmptyElement("p:grpSpPr");
            QString color=hexColor(s.fontColor,s.style==1?"FFFFFF":"202536");
            textShape(w,2,"Title","title",s.title,650000,500000,10800000,1600000,s.fontFamily,color,s.bold,s.italic,s.titleSize>0?s.titleSize:36);
            textShape(w,3,"Content","body",s.body,700000,2200000,10700000,3900000,s.fontFamily,color,s.bold,s.italic,s.bodySize>0?s.bodySize:22);
            int shapeId=4;
            for(const Element &e:s.elements){
                const qint64 x=qint64(e.x)*12700,y=qint64(e.y)*12700,cx=qint64(e.width)*12700,cy=qint64(e.height)*12700;
                if(e.type=="text")textShape(w,shapeId++,"Text box","",e.text,x,y,cx,cy,s.fontFamily,color,s.bold,s.italic,s.bodySize>0?s.bodySize:28);
                else if(e.type=="shape")basicShape(w,shapeId++,e.shape,e.fill,x,y,cx,cy);
                else imageShape(w,shapeId++,imageRelations.value(e.id),x,y,cx,cy);
            }
            w.writeEndElement();w.writeEndElement();w.writeEndElement();
        });
        parts["ppt/slides/_rels/slide"+QString::number(i+1)+".xml.rels"]=xml([&](QXmlStreamWriter &w){
            w.writeStartElement("Relationships");w.writeDefaultNamespace(packageRel);
            rel(w,"rId1","slideLayout","../slideLayouts/slideLayout1.xml");
            int imageIndex=0;
            for(const Element &e:s.elements)if(e.type=="image"){
                rel(w,imageRelations.value(e.id),"image","../media/image"+QString::number(mediaNumber-imageRelations.size()+ ++imageIndex)+".png");
            }
            w.writeEndElement();
        });
    }
    if(!OfficeZip::writePackage(path,parts)){setError("Could not export the PPTX file.");return false;}setError({});return true;
}

bool PresentDocument::importPptx(const QUrl &url){
    QString path=url.isLocalFile()?url.toLocalFile():QString();int error=0;
    zip_t *archive=path.isEmpty()?nullptr:zip_open(QFile::encodeName(path).constData(),ZIP_RDONLY,&error);
    if(!archive){setError("Could not open the PPTX file.");return false;}
    auto fail=[&](){zip_close(archive);setError("This PPTX file has unsupported or invalid slides.");return false;};
    QString presentation="ppt/presentation.xml";
    auto root=OfficeZip::relationships(OfficeZip::read(archive,"_rels/.rels"));
    if(!root.isEmpty())presentation=OfficeZip::resolve({},root.constBegin().value());
    QByteArray pres=OfficeZip::read(archive,presentation);if(pres.isEmpty())return fail();
    QXmlStreamReader pr(pres);QStringList ids;
    while(!pr.atEnd()){pr.readNext();if(pr.isStartElement()&&pr.name()==QLatin1StringView("sldId")){
        QString id=pr.attributes().value(rns,"id").toString();if(!id.isEmpty())ids.append(id);
    }}
    if(pr.hasError()||ids.isEmpty()||ids.size()>200)return fail();
    auto rels=OfficeZip::relationships(OfficeZip::read(archive,relPart(presentation)));
    QString folder=presentation.left(presentation.lastIndexOf('/'));QVector<Slide> loaded;
    for(QString id:ids){
        QString slidePath=OfficeZip::resolve(folder,rels.value(id));if(slidePath.isEmpty())return fail();
        QByteArray data=OfficeZip::read(archive,slidePath);if(data.isEmpty())return fail();
        auto slideRels=OfficeZip::relationships(OfficeZip::read(archive,relPart(slidePath)));
        QString slideFolder=slidePath.left(slidePath.lastIndexOf('/'));
        QXmlStreamReader r(data);Slide slide;
        int shapeCount=0;bool inShape=false,inPicture=false,inShapeProperties=false,titleFound=false;
        QString shapeType,shapeText,imageId,shapePreset,shapeFill;
        int shapeX=200,shapeY=150,shapeWidth=400,shapeHeight=180;
        auto geometry=[&](const QXmlStreamAttributes &attributes,bool extent){
            if(extent){shapeWidth=qRound(attributes.value("cx").toLongLong()/12700.0);shapeHeight=qRound(attributes.value("cy").toLongLong()/12700.0);}
            else{shapeX=qRound(attributes.value("x").toLongLong()/12700.0);shapeY=qRound(attributes.value("y").toLongLong()/12700.0);}
        };
        auto addText=[&](){
            if(shapeText.endsWith('\n'))shapeText.chop(1);
            if(shapeType.isEmpty()&&shapeText.isEmpty()&&!shapeFill.isEmpty()&&
               (shapePreset=="rect"||shapePreset=="ellipse")&&slide.elements.size()<30){
                Element element;element.id=QUuid::createUuid().toString(QUuid::WithoutBraces);element.type="shape";
                element.shape=shapePreset=="ellipse"?"ellipse":"rectangle";element.fill=shapeFill.toLower();
                element.x=std::clamp(shapeX,0,912);element.y=std::clamp(shapeY,0,508);
                element.width=std::clamp(shapeWidth,48,960-element.x);element.height=std::clamp(shapeHeight,32,540-element.y);
                slide.elements.append(element);return;
            }
            if(shapeType=="title"||shapeType=="ctrTitle"||(!titleFound&&shapeCount==1)){
                slide.title=shapeText;titleFound=true;
            }else if(shapeType=="body"||shapeType=="subTitle"){
                if(!slide.body.isEmpty())slide.body+='\n';slide.body+=shapeText;
            }else if(!shapeText.isEmpty()&&slide.elements.size()<30){
                Element element;element.id=QUuid::createUuid().toString(QUuid::WithoutBraces);element.type="text";
                element.x=std::clamp(shapeX,0,912);element.y=std::clamp(shapeY,0,508);
                element.width=std::clamp(shapeWidth,48,960-element.x);element.height=std::clamp(shapeHeight,32,540-element.y);
                element.text=shapeText;slide.elements.append(element);
            }
        };
        auto addPicture=[&](){
            QString part=OfficeZip::resolve(slideFolder,slideRels.value(imageId));if(part.isEmpty())return;
            QByteArray bytes=OfficeZip::read(archive,part,8*1024*1024);if(bytes.isEmpty())return;
            QBuffer source(&bytes);source.open(QIODevice::ReadOnly);QImageReader reader(&source);
            QSize original=reader.size();if(!original.isValid()||original.width()>10000||original.height()>10000)return;
            if(original.width()>1600||original.height()>900)reader.setScaledSize(original.scaled(1600,900,Qt::KeepAspectRatio));
            QImage image=reader.read();if(image.isNull())return;
            QByteArray png;QBuffer output(&png);output.open(QIODevice::WriteOnly);
            if(!image.save(&output,"PNG")||png.size()>3*1024*1024||slide.elements.size()>=30)return;
            Element element;element.id=QUuid::createUuid().toString(QUuid::WithoutBraces);element.type="image";element.image=std::move(png);
            element.x=std::clamp(shapeX,0,912);element.y=std::clamp(shapeY,0,508);
            element.width=std::clamp(shapeWidth,48,960-element.x);element.height=std::clamp(shapeHeight,32,540-element.y);
            slide.elements.append(element);
        };
        while(!r.atEnd()){
            r.readNext();
            if(r.isStartElement()){
                QString name=r.name().toString();
                if(r.namespaceUri()==pns&&name=="sp"){
                    inShape=true;shapeType.clear();shapeText.clear();shapePreset.clear();shapeFill.clear();shapeX=200;shapeY=150;shapeWidth=400;shapeHeight=180;++shapeCount;
                }else if(r.namespaceUri()==pns&&name=="pic"){
                    inPicture=true;imageId.clear();shapeX=200;shapeY=150;shapeWidth=400;shapeHeight=180;
                }else if(inShape&&r.namespaceUri()==pns&&name=="spPr")inShapeProperties=true;
                else if(inShape&&inShapeProperties&&r.namespaceUri()==ans&&name=="prstGeom")shapePreset=r.attributes().value("prst").toString();
                else if(inShape&&name=="ph")shapeType=r.attributes().value("type").toString();
                else if(inShape&&r.namespaceUri()==ans&&name=="t")shapeText+=r.readElementText();
                else if(inShape&&name=="rPr"){
                    auto a=r.attributes();if(a.value("b")=="1")slide.bold=true;if(a.value("i")=="1")slide.italic=true;
                    int size=a.value("sz").toInt()/100;
                    if(size>0){if(shapeType=="title"||shapeType=="ctrTitle")slide.titleSize=size;else slide.bodySize=size;}
                }else if(inShape&&name=="latin"){
                    QString family=r.attributes().value("typeface").toString();if(!family.isEmpty())slide.fontFamily=family;
                }else if((inShape||inPicture)&&r.namespaceUri()==ans&&name=="off")geometry(r.attributes(),false);
                else if((inShape||inPicture)&&r.namespaceUri()==ans&&name=="ext")geometry(r.attributes(),true);
                else if(inPicture&&r.namespaceUri()==ans&&name=="blip")imageId=r.attributes().value(rns,"embed").toString();
                else if(name=="srgbClr"){
                    QString color="#"+r.attributes().value("val").toString();
                    if(inShape&&inShapeProperties)shapeFill=color;
                    else if(inShape)slide.fontColor=color;
                    else if(!inPicture&&slide.backgroundColor.isEmpty())slide.backgroundColor=color;
                }
            }else if(r.isEndElement()){
                if(inShape&&r.namespaceUri()==pns&&r.name()==QLatin1StringView("spPr"))inShapeProperties=false;
                else if(inShape&&r.namespaceUri()==ans&&r.name()==QLatin1StringView("p"))shapeText+='\n';
                else if(inShape&&r.namespaceUri()==pns&&r.name()==QLatin1StringView("sp")){addText();inShape=false;}
                else if(inPicture&&r.namespaceUri()==pns&&r.name()==QLatin1StringView("pic")){addPicture();inPicture=false;}
            }
        }
        if(r.hasError())return fail();
        if(slide.backgroundColor.compare("#202536",Qt::CaseInsensitive)==0)slide.style=1;
        loaded.append(slide);
    }
    zip_close(archive);beginResetModel();m_slides=std::move(loaded);endResetModel();m_selected=0;
    m_path.clear();m_dirty=true;m_hasDocument=true;clearHistory();m_savedState.clear();removeSessionRecovery();
    m_recoveryTimer.start();setError({});emit selectionChanged();emit currentSlideChanged();emit stateChanged();return true;
}
