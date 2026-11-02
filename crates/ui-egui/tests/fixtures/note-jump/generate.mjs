// Usage: node worker/test/fixtures/note-jump/generate.mjs
// 각주·미주 "위치로 이동" 공용 시험 파일(note-jump.docx)을 이 폴더에 만든다. 서버(worker)와 워드 포크가 같은 파일로 같은 위치를 가리키는지 확인한다.
// 기대 위치는 expected.json에 손으로 적어 두었다(이 스크립트나 서버 코드가 만든 값이 아니다).
// 구성: 각주 id 1·2·3·4(본문 참조 순서는 3, 1, 4 — id 순서와 다름, 3은 두 번 참조, 2는 참조 안 됨), 각주 1 안에 표,
// separator·continuationSeparator 각주, 일반 노트 첫 문단 첫 run의 번호 표식(footnoteRef/endnoteRef; 각주 3은 표식 바로 뒤에 태그), 같은 태그 {{name}}이 여러 각주에 반복. 미주 id 1·2·3(참조 순서 2, 1 / 3은 참조 안 됨).
import { writeFileSync } from 'node:fs';
import { deflateRawSync, crc32 } from 'node:zlib';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const dir = path.dirname(fileURLToPath(import.meta.url));

function zip(entries) {
  const locals = [];
  const central = [];
  let offset = 0;
  for (const [name, content] of entries) {
    const raw = Buffer.from(content);
    const comp = deflateRawSync(raw, { level: 9 });
    const nameBuf = Buffer.from(name);
    const crc = crc32(raw);
    const lh = Buffer.alloc(30);
    lh.writeUInt32LE(0x04034b50, 0); lh.writeUInt16LE(20, 4); lh.writeUInt16LE(0x0800, 6); lh.writeUInt16LE(8, 8);
    lh.writeUInt32LE(crc, 14); lh.writeUInt32LE(comp.length, 18); lh.writeUInt32LE(raw.length, 22); lh.writeUInt16LE(nameBuf.length, 26);
    locals.push(lh, nameBuf, comp);
    const ch = Buffer.alloc(46);
    ch.writeUInt32LE(0x02014b50, 0); ch.writeUInt16LE(20, 4); ch.writeUInt16LE(20, 6); ch.writeUInt16LE(0x0800, 8);
    ch.writeUInt16LE(8, 10); ch.writeUInt32LE(crc, 16); ch.writeUInt32LE(comp.length, 20); ch.writeUInt32LE(raw.length, 24);
    ch.writeUInt16LE(nameBuf.length, 28); ch.writeUInt32LE(offset, 42);
    central.push(ch, nameBuf);
    offset += 30 + nameBuf.length + comp.length;
  }
  const cd = Buffer.concat(central);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0); end.writeUInt16LE(entries.length, 8); end.writeUInt16LE(entries.length, 10);
  end.writeUInt32LE(cd.length, 12); end.writeUInt32LE(offset, 16);
  return Buffer.concat([...locals, cd, end]);
}

const HEAD = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>';
const REL_NS = 'http://schemas.openxmlformats.org/package/2006/relationships';
const REL = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships';
const rels = (items) => `${HEAD}<Relationships xmlns="${REL_NS}">${items.map(([id, type, target]) => `<Relationship Id="${id}" Type="${REL}/${type}" Target="${target}"/>`).join('')}</Relationships>`;
const W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"';
const run = (t) => `<w:r><w:t xml:space="preserve">${t}</w:t></w:r>`;
const wp = (...parts) => `<w:p>${parts.join('')}</w:p>`;
const fnRef = (id) => `<w:r><w:footnoteReference w:id="${id}"/></w:r>`;
const enRef = (id) => `<w:r><w:endnoteReference w:id="${id}"/></w:r>`;
const tbl = (...cells) => `<w:tbl><w:tblPr><w:tblW w:w="4800" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="2400"/><w:gridCol w:w="2400"/></w:tblGrid><w:tr>${cells.map((c) => `<w:tc><w:tcPr><w:tcW w:w="2400" w:type="dxa"/></w:tcPr>${wp(run(c))}</w:tc>`).join('')}</w:tr></w:tbl>`;
const mark = (tag) => `<w:r><w:${tag}Ref/></w:r>`; // 노트 첫 문단 첫 run의 번호 표식(U+FFFC 한 글자로 센다)
const note = (tag, id, type, ...children) => `<w:${tag} ${type ? `w:type="${type}" ` : ''}w:id="${id}">${children.join('')}</w:${tag}>`;

// 본문: 각주는 3, 1, 3(다시), 4 순서로, 미주는 2, 1 순서로 참조한다.
const body = [
  wp(run('Body {{name}} start'), fnRef(3)),
  wp(run('Mid {{title}}'), fnRef(1), enRef(2)),
  wp(run('Again {{name}}'), fnRef(3), fnRef(4)),
  wp(run('Last'), enRef(1)),
].join('');

const footnotes = `${HEAD}<w:footnotes ${W}>${[
  note('footnote', -1, 'separator', wp('<w:r><w:separator/></w:r>', run('{{name}}'))),
  note('footnote', 0, 'continuationSeparator', wp('<w:r><w:continuationSeparator/></w:r>')),
  note('footnote', 1, '', wp(mark('footnote'), run('First {{name}}')), tbl('{{name}}', 'Tbl {{title}}'), '<w:p/>'),
  note('footnote', 2, '', wp(mark('footnote'), run('Unref {{name}} {{ghost}}'))),
  note('footnote', 3, '', wp(mark('footnote'), run('{{name}} Third')), wp(run('Second para {{fnkey}}'))),
  note('footnote', 4, '', wp(mark('footnote'), run('Four {{title}} {{name}}'))),
].join('')}</w:footnotes>`;

const endnotes = `${HEAD}<w:endnotes ${W}>${[
  note('endnote', -1, 'separator', wp('<w:r><w:separator/></w:r>')),
  note('endnote', 0, 'continuationSeparator', wp('<w:r><w:continuationSeparator/></w:r>')),
  note('endnote', 1, '', wp(mark('endnote'), run('Endnote one {{name}}'))),
  note('endnote', 2, '', wp(mark('endnote'), run('Endnote two {{name}} {{endkey}}'))),
  note('endnote', 3, '', wp(mark('endnote'), run('Endnote unref {{name}}'))),
].join('')}</w:endnotes>`;

const sect = '<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr>';
const CT = 'application/vnd.openxmlformats-officedocument.wordprocessingml';
writeFileSync(path.join(dir, 'note-jump.docx'), zip([
  ['[Content_Types].xml', `${HEAD}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="${CT}.document.main+xml"/><Override PartName="/word/footnotes.xml" ContentType="${CT}.footnotes+xml"/><Override PartName="/word/endnotes.xml" ContentType="${CT}.endnotes+xml"/></Types>`],
  ['_rels/.rels', rels([['rId1', 'officeDocument', 'word/document.xml']])],
  ['word/_rels/document.xml.rels', rels([['rId30', 'footnotes', 'footnotes.xml'], ['rId31', 'endnotes', 'endnotes.xml']])],
  ['word/document.xml', `${HEAD}<w:document ${W}><w:body>${body}${sect}</w:body></w:document>`],
  ['word/footnotes.xml', footnotes],
  ['word/endnotes.xml', endnotes],
]));
