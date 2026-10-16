//! IO-010: testes do pacote ZIP OOXML real e de seus limites.
use feather_io::{Codec, Documento, ErroIo, Registro, ooxml::{Xlsx, Docx, Pptx}};
use feather_tables::{Tabela, Celula, Regiao, Valor};
use std::io::{Cursor, Read};
fn exemplo() -> Tabela {
    let mut t = Tabela::nova(3, 3);
    for (l, c, v) in [
        (0, 0, Valor::Texto("Olá & <mundo>".into())),
        (0, 1, Valor::Numero(-2.5)),
        (1, 1, Valor::Logico(true)),
    ] {
        t.definir(l, c, Celula { valor: v, ..Default::default() }).unwrap();
    }
    t
}
fn parte(bytes: &[u8], caminho: &str) -> String {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut resultado = String::new();
    zip.by_name(caminho).unwrap().read_to_string(&mut resultado).unwrap();
    resultado
}
#[test]
fn xlsx_suporta_tipos_esparsos_utf8_e_mesclagem() {
    let mut t = exemplo();
    t.mesclar(Regiao { linha: 2, coluna: 0, linhas: 1, colunas: 3 }).unwrap();
    let bytes = Xlsx.gravar(&Documento::Tabela(t)).unwrap();
    let sheet = parte(&bytes, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains("Olá &amp; &lt;mundo&gt;"));
    assert!(sheet.contains("<v>-2.5</v>"));
    assert!(sheet.contains("t=\"b\"><v>1</v>"));
    assert!(sheet.contains("<mergeCell ref=\"A3:C3\"/>"));
    assert!(parte(&bytes, "[Content_Types].xml").contains("worksheet+xml"));
}
#[test]
fn docx_e_pptx_geram_pacotes_com_tabela() {
    for (ext, caminho, marcador) in [
        ("docx", "word/document.xml", "<w:tbl>"),
        ("pptx", "ppt/slides/slide1.xml", "<a:tbl>"),
    ] {
        let bytes = Registro::padrao().gravar(ext, &Documento::Tabela(exemplo())).unwrap();
        let xml = parte(&bytes, caminho);
        assert!(xml.contains(marcador));
        assert!(xml.contains("Olá &amp; &lt;mundo&gt;"));
    }
}
#[test]
fn rejeita_mesclagem_sem_suporte_e_numeros_invalidos() {
    let mut t = Tabela::nova(2, 2);
    t.mesclar(Regiao { linha: 0, coluna: 0, linhas: 2, colunas: 2 }).unwrap();
    for ext in ["docx", "pptx"] {
        assert!(matches!(Registro::padrao().gravar(ext, &Documento::Tabela(t.clone())), Err(ErroIo::DadosInvalidos(_))));
    }
    let mut t = Tabela::nova(1, 1);
    t.definir(0, 0, Celula { valor: Valor::Numero(f64::NAN), ..Default::default() }).unwrap();
    assert!(matches!(Xlsx.gravar(&Documento::Tabela(t)), Err(ErroIo::DadosInvalidos(_))));
}
#[test]
fn nao_anuncia_leitura_nem_imagem_grafico() {
    use feather_io::formatos::TipoDocumento;
    let registro = Registro::padrao();
    let catalogo = registro.catalogo();
    for formato in ["xlsx", "docx", "pptx"] {
        let info = catalogo.consultar(formato).unwrap();
        assert!(info.gravacao(TipoDocumento::Tabela));
        assert!(!info.leitura(TipoDocumento::Tabela));
        assert!(!info.gravacao(TipoDocumento::Grafico));
        assert!(!info.gravacao(TipoDocumento::Imagem));
        assert_eq!(registro.ler(formato, b""), Err(ErroIo::TipoIncompativel));
    }
}
#[test]
fn protege_grade_densa_e_caractere_xml_invalido() {
    let grande = Documento::Tabela(Tabela::nova(100_000, 10_000));
    assert_eq!(Docx.gravar(&grande), Err(ErroIo::LimiteExcedido));
    let mut t = Tabela::nova(1, 1);
    t.definir(0, 0, Celula { valor: Valor::Texto("abc\u{0001}".into()), ..Default::default() }).unwrap();
    assert!(matches!(Pptx.gravar(&Documento::Tabela(t)), Err(ErroIo::DadosInvalidos(_))));
}
