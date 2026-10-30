import zipfile, sys
out = sys.argv[1]
H = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"'
R = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships'
p = lambda t: f'<w:p><w:r><w:t xml:space="preserve">{t}</w:t></w:r></w:p>'
sect = lambda h, f, extra='': f'<w:sectPr><w:headerReference w:type="default" r:id="{h}"/><w:footerReference w:type="default" r:id="{f}"/>{extra}<w:pgSz w:w="11906" w:h="16838"/></w:sectPr>'
# section 1 refers to header2/footer2 first, the last section to header1/footer1.
img = '<w:r><w:drawing><wp:inline><wp:extent cx="100" cy="100"/><wp:docPr id="1" name="x"/></wp:inline></w:drawing></w:r>'
fld = '<w:fldSimple w:instr=" PAGE "><w:r><w:t>me</w:t></w:r></w:fldSimple>'
body = (
  f'<w:p><w:r><w:t>Sec1 {{{{h}}}}</w:t></w:r><w:pPr/></w:p>'
  f'<w:p><w:pPr>{sect("rId2","rId4")}</w:pPr><w:r><w:t>End1</w:t></w:r></w:p>'
  f'<w:p><w:r><w:t xml:space="preserve">Img {{{{na</w:t></w:r>{img}<w:r><w:t>me}}}}</w:t></w:r></w:p>'
  f'<w:p><w:r><w:t xml:space="preserve">Fld {{{{na</w:t></w:r>{fld}<w:r><w:t>}}}}</w:t></w:r></w:p>'
  f'<w:p><w:r><w:t>Plain {{{{name}}}}</w:t></w:r></w:p>'
  + sect("rId1","rId3"))
files = {
 '[Content_Types].xml': H+'<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
   +''.join(f'<Override PartName="/word/{k}{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.{k}+xml"/>' for k in ('header','footer') for n in (1,2))+'</Types>',
 '_rels/.rels': H+f'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{R}/officeDocument" Target="word/document.xml"/></Relationships>',
 'word/_rels/document.xml.rels': H+'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
   + f'<Relationship Id="rId1" Type="{R}/header" Target="header1.xml"/><Relationship Id="rId2" Type="{R}/header" Target="header2.xml"/>'
   + f'<Relationship Id="rId3" Type="{R}/footer" Target="footer1.xml"/><Relationship Id="rId4" Type="{R}/footer" Target="footer2.xml"/></Relationships>',
 'word/document.xml': H+f'<w:document {W}><w:body>{body}</w:body></w:document>',
 'word/header1.xml': H+f'<w:hdr {W}>{p("Header1 {{h}}")}</w:hdr>',
 'word/header2.xml': H+f'<w:hdr {W}>{p("Header2 {{h}}")}</w:hdr>',
 'word/footer1.xml': H+f'<w:ftr {W}>{p("Footer1 {{h}}")}</w:ftr>',
 'word/footer2.xml': H+f'<w:ftr {W}>{p("Footer2 {{h}}")}</w:ftr>',
}
with zipfile.ZipFile(out,'w',zipfile.ZIP_DEFLATED) as z:
    for k,v in files.items(): z.writestr(k,v)
