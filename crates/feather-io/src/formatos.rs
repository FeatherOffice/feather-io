//! Catálogo de formatos efetivamente suportados pelo registro de codecs.
//! Extensão reconhecida NÃO implica importação ou exportação implementada.

use std::collections::BTreeMap;

use crate::Documento;

/// Tipo de conteúdo neutro (não confundir com o formato externo do arquivo).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TipoDocumento {
    Tabela,
    Grafico,
    Imagem,
    Desenho,
}
impl TipoDocumento {
    pub const fn do_documento(documento: &Documento) -> Self {
        match documento {
            Documento::Tabela(_) => Self::Tabela,
            Documento::Grafico(_) => Self::Grafico,
            Documento::Imagem(_) => Self::Imagem,
            Documento::Desenho(_) => Self::Desenho,
        }
    }
}

/// Uma capacidade real informada pelo codec registrado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capacidade {
    pub tipo: TipoDocumento,
    pub ler: bool,
    pub gravar: bool,
}
impl Capacidade {
    pub const fn nova(tipo: TipoDocumento, ler: bool, gravar: bool) -> Self {
        Self { tipo, ler, gravar }
    }
}

/// Situação por extensão; `conhecido` não é sinônimo de `suportado`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Formato {
    pub extensao: String,
    pub reconhecido: bool,
    pub codec_registrado: bool,
    pub pode_ler: Vec<TipoDocumento>,
    pub pode_gravar: Vec<TipoDocumento>,
}
impl Formato {
    fn vazio(extensao: String, reconhecido: bool) -> Self {
        Self {
            extensao,
            reconhecido,
            codec_registrado: false,
            pode_ler: Vec::new(),
            pode_gravar: Vec::new(),
        }
    }
    /// Verdadeiro apenas quando existe codec com a capacidade específica.
    pub fn leitura(&self, tipo: TipoDocumento) -> bool {
        self.pode_ler.contains(&tipo)
    }
    pub fn gravacao(&self, tipo: TipoDocumento) -> bool {
        self.pode_gravar.contains(&tipo)
    }
    pub fn qualquer_capacidade(&self) -> bool {
        !self.pode_ler.is_empty() || !self.pode_gravar.is_empty()
    }
}

/// Nomes conhecidos pela suíte. Os codecs próprios dos apps ainda NÃO estão
/// registrados aqui. Novos codecs podem anunciar outras extensões.
pub const EXTENSOES_CONHECIDAS: &[&str] = &[
    "csv", "tsv", "xlsx", "ods", "docx", "odt", "pptx", "odp", "fshz", "fsdz", "fdoz", "fcdz",
    "png", "jpg", "jpeg", "webp", "gif", "bmp", "svg", "pdf", "zip",
];

/// Catálogo consultável pelo IO sem expor a estrutura interna do registro.
#[derive(Debug, Clone)]
pub struct Catalogo {
    formatos: BTreeMap<String, Formato>,
}
impl Default for Catalogo {
    fn default() -> Self {
        Self::novo()
    }
}
impl Catalogo {
    pub fn novo() -> Self {
        let mut formatos = BTreeMap::new();
        for &ext in EXTENSOES_CONHECIDAS {
            formatos.insert(ext.to_owned(), Formato::vazio(ext.to_owned(), true));
        }
        Self { formatos }
    }
    pub fn consultar(&self, extensao: &str) -> Option<&Formato> {
        let ext = normalizar_extensao(extensao);
        self.formatos.get(&ext)
    }
    pub fn iter(&self) -> impl Iterator<Item = &Formato> {
        self.formatos.values()
    }
    pub fn atualizar(&mut self, extensao: &str, capacidades: &[Capacidade]) {
        let ext = normalizar_extensao(extensao);
        if ext.is_empty() {
            return;
        }
        let conhecido = EXTENSOES_CONHECIDAS.contains(&ext.as_str());
        let mut formato = Formato::vazio(ext.clone(), conhecido);
        formato.codec_registrado = true;
        for &cap in capacidades {
            if cap.ler && !formato.pode_ler.contains(&cap.tipo) {
                formato.pode_ler.push(cap.tipo);
            }
            if cap.gravar && !formato.pode_gravar.contains(&cap.tipo) {
                formato.pode_gravar.push(cap.tipo);
            }
        }
        formato.pode_ler.sort();
        formato.pode_gravar.sort();
        self.formatos.insert(ext, formato);
    }
}
pub(crate) fn normalizar_extensao(extensao: &str) -> String {
    extensao.trim().trim_start_matches('.').to_ascii_lowercase()
}

#[cfg(test)]
mod testes {
    use super::*;
    #[test]
    fn catalogo_nao_confunde_formato_conhecido_com_suportado() {
        let c = Catalogo::novo();
        let docx = c.consultar("DOCX").unwrap();
        assert!(docx.reconhecido);
        assert!(!docx.codec_registrado);
        assert!(!docx.qualquer_capacidade());
        assert!(c.consultar(".fshz").unwrap().pode_ler.is_empty());
    }
    #[test]
    fn capacidade_e_declarada_por_tipo_sem_duplicacao() {
        let mut c = Catalogo::novo();
        c.atualizar(
            ".CSV",
            &[
                Capacidade::nova(TipoDocumento::Tabela, true, true),
                Capacidade::nova(TipoDocumento::Tabela, true, true),
            ],
        );
        let csv = c.consultar("csv").unwrap();
        assert_eq!(csv.pode_ler, vec![TipoDocumento::Tabela]);
        assert_eq!(csv.pode_gravar, vec![TipoDocumento::Tabela]);
        assert!(csv.codec_registrado);
        assert!(!csv.gravacao(TipoDocumento::Imagem));
    }
}
