//! Representação neutra e conversão consciente de perdas de atributos (IO-007).
//!
//! O teste de fidelidade usa o codec real: grava, relê e compara o resultado
//! com a estrutura em memória. Um codec sem leitura ou que falhe ao reler
//! **nunca** é considerado sem perdas por presunção.

use crate::{formatos::TipoDocumento, Documento, ErroIo, Registro};
use feather_tables::Tabela;

/// Tipo de informação alterada durante a gravação ou conversão.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CategoriaPerda {
    Dimensoes,
    Mesclagens,
    ValoresOuTipos,
    Estilos,
    EstruturaOuMetadados,
    FidelidadeNaoVerificada,
}

/// Diagnóstico explícito: deve ser mostrado ao usuário antes de aceitar
/// uma conversão destrutiva.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvisoPerda {
    pub categoria: CategoriaPerda,
    pub descricao: String,
}

impl AvisoPerda {
    fn novo(categoria: CategoriaPerda, descricao: &str) -> Self {
        Self {
            categoria,
            descricao: descricao.into(),
        }
    }
}

/// Bloquear é o padrão; permitir não oculta os avisos no resultado.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PoliticaConversao {
    #[default]
    BloquearPerdas,
    PermitirComAvisos,
}

/// Documento neutro carregado de um codec, com sua proveniência preservada.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentoIntermediario {
    pub formato_origem: String,
    pub tipo: TipoDocumento,
    pub documento: Documento,
}

impl DocumentoIntermediario {
    pub fn novo(origem: &str, documento: Documento) -> Self {
        let tipo = TipoDocumento::do_documento(&documento);
        Self {
            formato_origem: origem.trim().trim_start_matches('.').to_ascii_lowercase(),
            tipo,
            documento,
        }
    }
}

/// A confirmação permissiva retorna bytes *e* avisos; nunca retorna apenas
/// os bytes e descarta os diagnósticos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultadoConversao {
    pub bytes: Vec<u8>,
    pub formato_destino: String,
    pub avisos: Vec<AvisoPerda>,
}
impl ResultadoConversao {
    pub fn sem_perdas_verificadas(&self) -> bool {
        self.avisos.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroConversao {
    Io(ErroIo),
    PerdasDetectadas(Vec<AvisoPerda>),
}
impl From<ErroIo> for ErroConversao {
    fn from(e: ErroIo) -> Self {
        Self::Io(e)
    }
}

fn perda_nao_verificada() -> AvisoPerda {
    AvisoPerda::novo(
        CategoriaPerda::FidelidadeNaoVerificada,
        "O codec de destino não permitiu comprovar a fidelidade por releitura.",
    )
}

/// Analisa alterações reais sem pressupor compatibilidade de tipos de arquivo.
/// Não percorre células vazias de tabelas enormes: o modelo é esparso.
pub fn comparar_documentos(origem: &Documento, relido: &Documento) -> Vec<AvisoPerda> {
    if origem == relido {
        return Vec::new();
    }
    let mut avisos = Vec::new();
    match (origem, relido) {
        (Documento::Tabela(a), Documento::Tabela(b)) => comparar_tabelas(a, b, &mut avisos),
        _ => avisos.push(AvisoPerda::novo(
            CategoriaPerda::EstruturaOuMetadados,
            "O conteúdo ou o tipo do documento mudou após a exportação.",
        )),
    }
    avisos
}

fn comparar_tabelas(a: &Tabela, b: &Tabela, avisos: &mut Vec<AvisoPerda>) {
    if a.dimensoes() != b.dimensoes() {
        avisos.push(AvisoPerda::novo(
            CategoriaPerda::Dimensoes,
            "A quantidade de linhas ou colunas mudou.",
        ));
    }
    if a.mesclagens() != b.mesclagens() {
        avisos.push(AvisoPerda::novo(
            CategoriaPerda::Mesclagens,
            "Uma ou mais regiões mescladas não foram preservadas.",
        ));
    }
    let mut valores = false;
    let mut estilos = false;
    // Os iteradores são esparsos; verificar ambas as direções detecta
    // inclusões e exclusões de células, sem materializar uma grade densa.
    for (&(l, c), original) in a.iter() {
        match b.obter(l, c) {
            Some(destino) => {
                valores |= original.valor != destino.valor;
                estilos |= original.estilo != destino.estilo;
            }
            None => {
                valores |= original.valor != Default::default();
                estilos |= original.estilo != Default::default();
            }
        }
    }
    for (&(l, c), destino) in b.iter() {
        if a.obter(l, c).is_none() {
            valores |= destino.valor != Default::default();
            estilos |= destino.estilo != Default::default();
        }
    }
    if valores {
        avisos.push(AvisoPerda::novo(
            CategoriaPerda::ValoresOuTipos,
            "Valores, tipos de célula ou conteúdo foram alterados.",
        ));
    }
    if estilos {
        avisos.push(AvisoPerda::novo(
            CategoriaPerda::Estilos,
            "A formatação de uma ou mais células não foi preservada.",
        ));
    }
    // A Tabela inclui larguras, alturas, padding, layout e política de bordas
    // que podem não estar publicamente enumeráveis. A comparação estrutural
    // completa cobre também esses atributos sem um scan denso.
    if avisos.is_empty() && a != b {
        avisos.push(AvisoPerda::novo(
            CategoriaPerda::EstruturaOuMetadados,
            "Dimensões personalizadas, layout ou outros metadados foram alterados.",
        ));
    }
}

impl Registro {
    /// Carrega uma representação neutra com formato e tipo explicitados.
    pub fn ler_intermediario(
        &self,
        origem: &str,
        bytes: &[u8],
    ) -> Result<DocumentoIntermediario, ErroIo> {
        let doc = self.ler(origem, bytes)?;
        Ok(DocumentoIntermediario::novo(origem, doc))
    }

    /// Exporta após verificar fidelidade: se houver risco, bloqueia ou retorna
    /// os avisos junto dos bytes segundo a política solicitada.
    pub fn gravar_com_politica(
        &self,
        destino: &str,
        documento: &Documento,
        politica: PoliticaConversao,
    ) -> Result<ResultadoConversao, ErroConversao> {
        let tipo = TipoDocumento::do_documento(documento);
        let destino_codec = self.codec(destino)?;
        let catalogo = self.catalogo();
        let formato = catalogo
            .consultar(destino)
            .ok_or_else(|| ErroIo::FormatoDesconhecido(destino.to_owned()))?;
        if !formato.gravacao(tipo) {
            return Err(ErroIo::TipoIncompativel.into());
        }
        let bytes = destino_codec.gravar(documento)?;

        // Ausência de capacidade de leitura, falha ou tipo incompatível
        // tornam o round-trip inconclusivo: avisar, jamais presumir sucesso.
        let avisos = if formato.leitura(tipo) {
            match destino_codec.ler(&bytes) {
                Ok(relido) => comparar_documentos(documento, &relido),
                Err(_) => vec![perda_nao_verificada()],
            }
        } else {
            vec![perda_nao_verificada()]
        };
        if !avisos.is_empty() && politica == PoliticaConversao::BloquearPerdas {
            return Err(ErroConversao::PerdasDetectadas(avisos));
        }
        Ok(ResultadoConversao {
            bytes,
            formato_destino: destino.trim().trim_start_matches('.').to_ascii_lowercase(),
            avisos,
        })
    }

    /// Conversão explícita via modelo intermediário com avisos verificáveis.
    pub fn converter_com_politica(
        &self,
        origem: &str,
        destino: &str,
        bytes: &[u8],
        politica: PoliticaConversao,
    ) -> Result<ResultadoConversao, ErroConversao> {
        let intermediario = self.ler_intermediario(origem, bytes)?;
        if !self.pode_converter(origem, destino, intermediario.tipo) {
            return Err(ErroIo::TipoIncompativel.into());
        }
        self.gravar_com_politica(destino, &intermediario.documento, politica)
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::{Codec, Csv};
    use feather_tables::{Celula, Regiao, Valor};

    fn tabela_estilizada() -> Documento {
        let mut t = Tabela::nova(3, 2);
        let mut c = Celula {
            valor: Valor::Numero(42.0),
            ..Default::default()
        };
        c.estilo.negrito = true;
        t.definir(0, 0, c).unwrap();
        t.mesclar(Regiao {
            linha: 1,
            coluna: 0,
            linhas: 2,
            colunas: 1,
        })
        .unwrap();
        t.definir_largura(1, 120.0).unwrap();
        Documento::Tabela(t)
    }

    #[test]
    fn texto_csv_nao_produz_alerta_falso() {
        let reg = Registro::padrao();
        let fonte = b"nome,valor\nA,10";
        let saida = reg
            .converter_com_politica("csv", "csv", fonte, PoliticaConversao::BloquearPerdas)
            .unwrap();
        assert!(saida.sem_perdas_verificadas());
        assert_eq!(saida.bytes, fonte);
        let intermediario = reg.ler_intermediario(".CSV", fonte).unwrap();
        assert_eq!(intermediario.tipo, TipoDocumento::Tabela);
        assert_eq!(intermediario.formato_origem, "csv");
    }

    #[test]
    fn bloqueia_formato_que_perde_estilos_mesclagens_e_tipos() {
        let reg = Registro::padrao();
        let original = tabela_estilizada();
        let Err(ErroConversao::PerdasDetectadas(avisos)) =
            reg.gravar_com_politica("csv", &original, PoliticaConversao::BloquearPerdas)
        else {
            panic!("CSV não preserva tabela estilizada");
        };
        for categoria in [
            CategoriaPerda::ValoresOuTipos,
            CategoriaPerda::Estilos,
            CategoriaPerda::Mesclagens,
        ] {
            assert!(
                avisos.iter().any(|a| a.categoria == categoria),
                "{categoria:?}"
            );
        }
    }

    #[test]
    fn modo_permissivo_exige_escolha_e_entrega_diagnosticos() {
        let reg = Registro::padrao();
        let resultado = reg
            .gravar_com_politica(
                "csv",
                &tabela_estilizada(),
                PoliticaConversao::PermitirComAvisos,
            )
            .unwrap();
        assert!(!resultado.bytes.is_empty());
        assert!(!resultado.sem_perdas_verificadas());
        assert_eq!(resultado.formato_destino, "csv");
        assert!(resultado
            .avisos
            .iter()
            .any(|a| a.categoria == CategoriaPerda::Mesclagens));
    }

    #[test]
    fn detecta_tambem_perda_de_metadados_sem_iterar_grade_inteira() {
        let mut grande = Tabela::nova(2_000_000, 10);
        grande.definir_largura(4, 82.0).unwrap();
        let outro = Tabela::nova(2_000_000, 10);
        let avisos = comparar_documentos(&Documento::Tabela(grande), &Documento::Tabela(outro));
        assert_eq!(avisos.len(), 1);
        assert_eq!(avisos[0].categoria, CategoriaPerda::EstruturaOuMetadados);
    }

    /// Codec gravável sem releitura: nunca pode afirmar que não perdeu dados.
    struct SomenteEscrita;
    impl Codec for SomenteEscrita {
        fn extensoes(&self) -> &'static [&'static str] {
            &["mock"]
        }
        fn capacidades(&self) -> &'static [crate::formatos::Capacidade] {
            const C: [crate::formatos::Capacidade; 1] = [crate::formatos::Capacidade::nova(
                TipoDocumento::Tabela,
                false,
                true,
            )];
            &C
        }
        fn ler(&self, _: &[u8]) -> Result<Documento, ErroIo> {
            Err(ErroIo::TipoIncompativel)
        }
        fn gravar(&self, _: &Documento) -> Result<Vec<u8>, ErroIo> {
            Ok(b"mock".to_vec())
        }
    }

    #[test]
    fn codec_sem_leitura_gera_alerta_inconclusivo() {
        let mut reg = Registro::novo();
        reg.registrar(SomenteEscrita);
        let original = Documento::Tabela(Tabela::nova(0, 0));
        assert!(matches!(
            reg.gravar_com_politica("mock", &original, PoliticaConversao::BloquearPerdas),
            Err(ErroConversao::PerdasDetectadas(ref avisos))
            if avisos[0].categoria == CategoriaPerda::FidelidadeNaoVerificada
        ));
        let permissivo = reg
            .gravar_com_politica("mock", &original, PoliticaConversao::PermitirComAvisos)
            .unwrap();
        assert_eq!(permissivo.bytes, b"mock");
        assert_eq!(
            permissivo.avisos[0].categoria,
            CategoriaPerda::FidelidadeNaoVerificada
        );
    }

    #[test]
    fn contrato_legado_de_csv_continua_funcional() {
        let registro = Registro::padrao();
        assert_eq!(
            registro.converter("csv", "csv", b"a,b\n1,2").unwrap(),
            b"a,b\n1,2"
        );
        // Evitar importação não utilizada: Codec está acima para o mock.
        assert_eq!(Csv.extensoes(), &["csv"]);
    }
}
