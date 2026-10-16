//! Exportação OOXML opt-in (IO-010), sem substituir os codecs nativos.
use std::io::{Cursor, Write};
use feather_tables::{Tabela, Valor};
use crate::{Codec, Documento, ErroIo, formatos::{Capacidade, TipoDocumento}};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

const CAP_TABELA_SAIDA: [Capacidade; 1] = [Capacidade::nova(TipoDocumento::Tabela, false, true)];
const MAX_ARQUIVO: usize = 128 * 1024 * 1024;
const MAX_CELULAS: usize = 100_000;

#[derive(Debug, Default, Clone, Copy)]
pub struct Xlsx;
#[derive(Debug, Default, Clone, Copy)]
pub struct Docx;
#[derive(Debug, Default, Clone, Copy)]
pub struct Pptx;
macro_rules! codec_tabela {
    ($tipo:ident, $ext:literal, $gerador:ident) => {
        impl Codec for $tipo {
            fn extensoes(&self) -> &'static [&'static str] { &[$ext] }
            fn capacidades(&self) -> &'static [Capacidade] { &CAP_TABELA_SAIDA }
            fn ler(&self, _: &[u8]) -> Result<Documento, ErroIo> {
                Err(ErroIo::TipoIncompativel)
            }
            fn gravar(&self, documento: &Documento) -> Result<Vec<u8>, ErroIo> {
                let Documento::Tabela(tabela) = documento else {
                    return Err(ErroIo::TipoIncompativel);
                };
                $gerador(tabela)
            }
        }
    };
}
codec_tabela!(Xlsx, "xlsx", xlsx);
codec_tabela!(Docx, "docx", docx);
codec_tabela!(Pptx, "pptx", pptx);

fn validar(tabela: &Tabela, limite_linhas: usize, limite_colunas: usize, denso: bool) -> Result<(), ErroIo> {
    let (linhas, colunas) = tabela.dimensoes();
    if linhas > limite_linhas || colunas > limite_colunas || tabela.iter().count() > MAX_CELULAS {
        return Err(ErroIo::LimiteExcedido);
    }
    if denso && linhas.checked_mul(colunas).map_or(true, |n| n > MAX_CELULAS) {
        return Err(ErroIo::LimiteExcedido);
    }
    if denso && (linhas == 0 || colunas == 0 || !tabela.mesclagens().is_empty()) {
        return Err(ErroIo::DadosInvalidos("DOCX/PPTX exigem tabela não vazia e ainda não exportam células mescladas".into()));
    }
    for (_, celula) in tabela.iter() {
        match &celula.valor {
            Valor::Numero(v) if !v.is_finite() => return Err(ErroIo::DadosInvalidos("número não finito".into())),
            Valor::Texto(t) => {
                validar_xml(t)?;
                if t.chars().count() > 32767 { return Err(ErroIo::LimiteExcedido); }
            }
            _ => {}
        }
    }
    Ok(())
}
fn validar_xml(texto: &str) -> Result<(), ErroIo> {
    if texto.chars().any(|c| !matches!(c as u32, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)) {
        return Err(ErroIo::DadosInvalidos("caractere inválido para XML".into()));
    }
    Ok(())
}
fn escapar(texto: &str) -> String {
    texto.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
        .replace('"', "&quot;").replace('\'', "&apos;")
}
fn zipar(entradas: &[(&str, String)]) -> Result<Vec<u8>, ErroIo> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let opcoes = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (nome, conteudo) in entradas {
        if conteudo.len() > MAX_ARQUIVO { return Err(ErroIo::LimiteExcedido); }
        writer.start_file(*nome, opcoes).map_err(|e| ErroIo::DadosInvalidos(e.to_string()))?;
        writer.write_all(conteudo.as_bytes()).map_err(|e| ErroIo::DadosInvalidos(e.to_string()))?;
    }
    let bytes = writer.finish().map_err(|e| ErroIo::DadosInvalidos(e.to_string()))?.into_inner();
    if bytes.len() > MAX_ARQUIVO { return Err(ErroIo::LimiteExcedido); }
    Ok(bytes)
}
fn col_excel(mut indice: usize) -> String {
    let mut saida = String::new();
    loop {
        saida.insert(0, (b'A' + (indice % 26) as u8) as char);
        if indice < 26 { break; }
        indice = indice / 26 - 1;
    }
    saida
}
fn ref_excel(linha: usize, coluna: usize) -> String {
    format!("{}{}", col_excel(coluna), linha + 1)
}
fn rel(alvo: &str, tipo: &str) -> String {
    format!(r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{tipo}" Target="{alvo}"/></Relationships>"#)
}

fn xlsx(tabela: &Tabela) -> Result<Vec<u8>, ErroIo> {
    validar(tabela, 1_048_576, 16_384, false)?;
    let (linhas, colunas) = tabela.dimensoes();
    let mut sheet = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#);
    let canto = if linhas == 0 || colunas == 0 { "A1".to_owned() } else { ref_excel(linhas - 1, colunas - 1) };
    sheet.push_str(&format!(r#"<dimension ref="A1:{canto}"/><sheetData>"#));
    let mut linha_anterior = None;
    for (&(l, c), celula) in tabela.iter() {
        if linha_anterior != Some(l) {
            if linha_anterior.is_some() { sheet.push_str("</row>"); }
            sheet.push_str(&format!(r#"<row r="{}">"#, l + 1));
            linha_anterior = Some(l);
        }
        let referencia = ref_excel(l, c);
        match &celula.valor {
            Valor::Vazio => {},
            Valor::Texto(s) => sheet.push_str(&format!(r#"<c r="{referencia}" t="inlineStr"><is><t xml:space="preserve">{}</t></is></c>"#, escapar(s))),
            Valor::Numero(n) => sheet.push_str(&format!(r#"<c r="{referencia}"><v>{n}</v></c>"#)),
            Valor::Logico(v) => sheet.push_str(&format!(r#"<c r="{referencia}" t="b"><v>{}</v></c>"#, u8::from(*v))),
        }
    }
    if linha_anterior.is_some() { sheet.push_str("</row>"); }
    sheet.push_str("</sheetData>");
    if !tabela.mesclagens().is_empty() {
        sheet.push_str(&format!(r#"<mergeCells count="{}">"#, tabela.mesclagens().len()));
        for m in tabela.mesclagens() {
            let inicio = ref_excel(m.linha, m.coluna);
            let fim = ref_excel(m.linha + m.linhas - 1, m.coluna + m.colunas - 1);
            sheet.push_str(&format!(r#"<mergeCell ref="{inicio}:{fim}"/>"#));
        }
        sheet.push_str("</mergeCells>");
    }
    sheet.push_str("</worksheet>");
    zipar(&[
        ("[Content_Types].xml", r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#.into()),
        ("_rels/.rels", rel("xl/workbook.xml", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument")),
        ("xl/workbook.xml", r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="FeatherOffice" sheetId="1" r:id="rId1"/></sheets></workbook>"#.into()),
        ("xl/_rels/workbook.xml.rels", rel("worksheets/sheet1.xml", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet")),
        ("xl/worksheets/sheet1.xml", sheet),
    ])
}
fn texto_valor(valor: &Valor) -> String {
    match valor {
        Valor::Vazio => String::new(),
        Valor::Texto(v) => escapar(v),
        Valor::Numero(n) => n.to_string(),
        Valor::Logico(b) => if *b { "VERDADEIRO".into() } else { "FALSO".into() },
    }
}
fn docx(tabela: &Tabela) -> Result<Vec<u8>, ErroIo> {
    validar(tabela, 10_000, 256, true)?;
    let (linhas, colunas) = tabela.dimensoes();
    let mut document = String::from(r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid>"#);
    for _ in 0..colunas { document.push_str(r#"<w:gridCol w:w="1440"/>"#); }
    document.push_str("</w:tblGrid>");
    for l in 0..linhas {
        document.push_str("<w:tr>");
        for c in 0..colunas {
            document.push_str("<w:tc><w:tcPr/><w:p><w:r><w:t xml:space=\"preserve\">");
            if let Some(celula) = tabela.obter(l, c) { document.push_str(&texto_valor(&celula.valor)); }
            document.push_str("</w:t></w:r></w:p></w:tc>");
        }
        document.push_str("</w:tr>");
    }
    document.push_str("</w:tbl><w:sectPr/></w:body></w:document>");
    zipar(&[
        ("[Content_Types].xml", r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.into()),
        ("_rels/.rels", rel("word/document.xml", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument")),
        ("word/document.xml", document),
    ])
}
fn pptx(tabela: &Tabela) -> Result<Vec<u8>, ErroIo> {
    validar(tabela, 50, 30, true)?;
    let (linhas, colunas) = tabela.dimensoes();
    let largura = 10_500_000 / colunas;
    let altura = 5_500_000 / linhas;
    let mut slide = String::from(r#"<?xml version="1.0" encoding="UTF-8"?><p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr><p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="2" name="Tabela FeatherOffice"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="850000" y="650000"/><a:ext cx="10500000" cy="5500000"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table"><a:tbl><a:tblPr firstRow="0"/><a:tblGrid>"#);
    for _ in 0..colunas { slide.push_str(&format!(r#"<a:gridCol w="{largura}"/>"#)); }
    slide.push_str("</a:tblGrid>");
    for l in 0..linhas {
        slide.push_str(&format!(r#"<a:tr h="{altura}">"#));
        for c in 0..colunas {
            slide.push_str("<a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"pt-BR\"/><a:t>");
            if let Some(celula) = tabela.obter(l, c) { slide.push_str(&texto_valor(&celula.valor)); }
            slide.push_str("</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc>");
        }
        slide.push_str("</a:tr>");
    }
    slide.push_str("</a:tbl></a:graphicData></a:graphic></p:graphicFrame></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>");
    let tree = r#"<p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr></p:spTree>"#;
    let master = format!(r#"<?xml version="1.0" encoding="UTF-8"?><p:sldMaster xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld>{tree}</p:cSld><p:clrMap accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" bg1="lt1" bg2="lt2" folHlink="folHlink" hlink="hlink" tx1="dk1" tx2="dk2"/><p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst></p:sldMaster>"#);
    let layout = format!(r#"<?xml version="1.0" encoding="UTF-8"?><p:sldLayout xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" type="blank" preserve="1"><p:cSld name="Em branco">{tree}</p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#);
    let theme = r#"<?xml version="1.0" encoding="UTF-8"?><a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="FeatherOffice"><a:themeElements><a:clrScheme name="Padrão"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="1F1F1F"/></a:dk2><a:lt2><a:srgbClr val="F0F0F0"/></a:lt2><a:accent1><a:srgbClr val="4472C4"/></a:accent1><a:accent2><a:srgbClr val="ED7D31"/></a:accent2><a:accent3><a:srgbClr val="A5A5A5"/></a:accent3><a:accent4><a:srgbClr val="FFC000"/></a:accent4><a:accent5><a:srgbClr val="5B9BD5"/></a:accent5><a:accent6><a:srgbClr val="70AD47"/></a:accent6><a:hlink><a:srgbClr val="0563C1"/></a:hlink><a:folHlink><a:srgbClr val="954F72"/></a:folHlink></a:clrScheme><a:fontScheme name="FeatherOffice"><a:majorFont><a:latin typeface="Arial"/></a:majorFont><a:minorFont><a:latin typeface="Arial"/></a:minorFont></a:fontScheme><a:fmtScheme name="FeatherOffice"><a:fillStyleLst><a:solidFill><a:schemeClr val="accent1"/></a:solidFill><a:solidFill><a:schemeClr val="accent2"/></a:solidFill><a:solidFill><a:schemeClr val="accent3"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="12700"><a:solidFill><a:schemeClr val="accent1"/></a:solidFill></a:ln><a:ln w="12700"><a:solidFill><a:schemeClr val="accent2"/></a:solidFill></a:ln><a:ln w="12700"><a:solidFill><a:schemeClr val="accent3"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="lt1"/></a:solidFill><a:solidFill><a:schemeClr val="lt2"/></a:solidFill><a:solidFill><a:schemeClr val="accent1"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>"#;
    zipar(&[
        ("[Content_Types].xml", r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/><Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/><Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/></Types>"#.into()),
        ("_rels/.rels", rel("ppt/presentation.xml", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument")),
        ("ppt/presentation.xml", r#"<?xml version="1.0" encoding="UTF-8"?><p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst><p:sldIdLst><p:sldId id="256" r:id="rId2"/></p:sldIdLst><p:sldSz cx="12192000" cy="6858000" type="screen16x9"/><p:notesSz cx="6858000" cy="9144000"/></p:presentation>"#.into()),
        ("ppt/_rels/presentation.xml.rels", r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/></Relationships>"#.into()),
        ("ppt/slides/slide1.xml", slide),
        ("ppt/slides/_rels/slide1.xml.rels", rel("../slideLayouts/slideLayout1.xml", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout")),
        ("ppt/slideLayouts/slideLayout1.xml", layout),
        ("ppt/slideLayouts/_rels/slideLayout1.xml.rels", rel("../slideMasters/slideMaster1.xml", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster")),
        ("ppt/slideMasters/slideMaster1.xml", master),
        ("ppt/slideMasters/_rels/slideMaster1.xml.rels", r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/></Relationships>"#.into()),
        ("ppt/theme/theme1.xml", theme.into()),
    ])
}
