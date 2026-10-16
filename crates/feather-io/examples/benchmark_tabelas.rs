//! TBL-014: benchmark reproduzível da tabela esparsa, grade densa e CSV.
//! NÃO serializa nem materializa uma grade de centenas de milhões de células.
//! Sem dependências adicionais; mede tempos e payload real. Memória da grade
//! densa é uma estimativa; RSS de processo no Linux não é tamanho de BTreeMap.
use feather_io::{Documento, Registro};
use feather_tables::{Celula, Tabela, Valor};
use std::hint::black_box;
use std::mem::size_of;
use std::time::{Duration, Instant};

const LIMITE_DENSO_CSV: usize = 100_000;
const CONSULTAS: usize = 10_000;

#[derive(Clone, Copy, Debug)]
struct Cenario {
    nome: &'static str,
    linhas: usize,
    colunas: usize,
    ocupadas: usize,
}
impl Cenario {
    fn total(self) -> usize {
        self.linhas.checked_mul(self.colunas).unwrap()
    }
    fn pequeno(self) -> bool {
        self.total() <= LIMITE_DENSO_CSV
    }
}
const DOC: Cenario = Cenario {
    nome: "documento",
    linhas: 800,
    colunas: 32,
    ocupadas: 320,
};
const MASSIVO: Cenario = Cenario {
    nome: "esparsa_gigante",
    linhas: 1_000_000,
    colunas: 512,
    ocupadas: 3_200,
};

fn indice(i: usize, total: usize) -> usize {
    ((i as u64 * 2_654_435_761 + 1_013_904_223) % total as u64) as usize
}
fn celula(i: usize) -> Celula {
    Celula {
        valor: Valor::Numero((i + 1) as f64),
        ..Default::default()
    }
}
fn preencher(c: Cenario) -> Tabela {
    assert!(c.ocupadas < c.total());
    let mut t = Tabela::nova(c.linhas, c.colunas);
    for i in 0..c.ocupadas {
        let n = indice(i, c.total());
        t.definir(n / c.colunas, n % c.colunas, celula(i)).unwrap();
    }
    assert_eq!(t.iter().count(), c.ocupadas);
    t
}
/// Grade densa somente para documentos pequenos: o cenário gigante NÃO aloca.
fn densa_pequena(c: Cenario) -> Option<Vec<Option<Celula>>> {
    if !c.pequeno() {
        return None;
    }
    let mut densa = vec![None; c.total()];
    for i in 0..c.ocupadas {
        densa[indice(i, c.total())] = Some(celula(i));
    }
    Some(densa)
}
fn rss_kib() -> Option<u64> {
    let arquivo = std::fs::read_to_string("/proc/self/status").ok()?;
    let campo = arquivo.lines().find(|s| s.starts_with("VmRSS:"))?;
    campo.split_whitespace().nth(1)?.parse().ok()
}
#[derive(Debug)]
struct Medidas {
    c: Cenario,
    ms_criacao: f64,
    ms_consulta: f64,
    ms_densa: Option<f64>,
    ms_consulta_densa: Option<f64>,
    estimativa_densa_bytes: u128,
    alocacao_densa_bytes: Option<usize>,
    csv_bytes: Option<usize>,
    ms_csv: Option<f64>,
    rss_antes: Option<u64>,
    rss_esparsa: Option<u64>,
    rss_densa: Option<u64>,
}
fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}
fn medir(c: Cenario) -> Medidas {
    let rss_antes = rss_kib();
    let start = Instant::now();
    let tabela = preencher(c);
    let ms_criacao = ms(start.elapsed());
    let rss_esparsa = rss_kib();

    let start = Instant::now();
    let mut encontrados = 0;
    for i in 0..CONSULTAS {
        let n = indice(i + c.ocupadas + 42, c.total());
        encontrados += usize::from(black_box(
            tabela.obter(n / c.colunas, n % c.colunas).is_some(),
        ));
    }
    let ms_consulta = ms(start.elapsed());

    let start = Instant::now();
    let grade = densa_pequena(c);
    let ms_densa = grade.as_ref().map(|_| ms(start.elapsed()));
    let rss_densa = grade.as_ref().and_then(|_| rss_kib());
    let alocacao_densa_bytes = grade
        .as_ref()
        .map(|d| d.capacity() * size_of::<Option<Celula>>());
    let ms_consulta_densa = grade.as_ref().map(|d| {
        let start = Instant::now();
        let mut encontrado = 0;
        for i in 0..CONSULTAS {
            let n = indice(i + c.ocupadas + 42, c.total());
            encontrado += usize::from(black_box(d[n].is_some()));
        }
        assert_eq!(encontrado, encontrados, "modelos divergentes nas consultas");
        ms(start.elapsed())
    });
    black_box(&grade);

    // CSV é inerentemente denso: apenas a tabela pequena vira payload.
    let (csv_bytes, ms_csv) = if c.pequeno() {
        let start = Instant::now();
        let bytes = Registro::padrao()
            .gravar("csv", &Documento::Tabela(tabela))
            .unwrap();
        let tempo = ms(start.elapsed());
        (Some(bytes.len()), Some(tempo))
    } else {
        assert!(grade.is_none(), "NUNCA materializar grade grande");
        assert_eq!(tabela.iter().count(), c.ocupadas);
        (None, None)
    };
    Medidas {
        c,
        ms_criacao,
        ms_consulta,
        ms_densa,
        ms_consulta_densa,
        estimativa_densa_bytes: (c.total() as u128) * (size_of::<Option<Celula>>() as u128),
        alocacao_densa_bytes,
        csv_bytes,
        ms_csv,
        rss_antes,
        rss_esparsa,
        rss_densa,
    }
}
fn opcao<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map(|x| x.to_string()).unwrap_or_else(|| "NA".into())
}
fn imprimir(m: &Medidas) {
    println!(
        "{},{},{},{},{:.7},{:.3},{:.3},{},{},{},{},{},{},{},{},{}",
        m.c.nome,
        m.c.linhas,
        m.c.colunas,
        m.c.ocupadas,
        m.c.ocupadas as f64 / m.c.total() as f64,
        m.ms_criacao,
        m.ms_consulta,
        opcao(m.ms_densa),
        opcao(m.ms_consulta_densa),
        m.estimativa_densa_bytes,
        opcao(m.alocacao_densa_bytes),
        opcao(m.csv_bytes),
        opcao(m.ms_csv),
        opcao(m.rss_antes),
        opcao(m.rss_esparsa),
        opcao(m.rss_densa)
    );
}
fn main() {
    println!("cenario,linhas,colunas,ocupadas,densidade,criar_esparsa_ms,consultas_esparsa_ms,criar_densa_ms,consultas_densa_ms,estimativa_densa_bytes,densa_alocada_bytes,payload_csv_bytes,gerar_csv_ms,rss_inicial_kib,rss_esparsa_kib,rss_densa_kib");
    for c in [DOC, MASSIVO] {
        imprimir(&medir(c));
    }
}
#[cfg(test)]
mod testes {
    use super::*;
    #[test]
    fn cenario_gigante_permanece_esparso() {
        assert_eq!(MASSIVO.total(), 512_000_000);
        assert!(!MASSIVO.pequeno());
        assert!(densa_pequena(MASSIVO).is_none());
        let t = preencher(MASSIVO);
        assert_eq!(t.iter().count(), 3_200);
        assert_eq!(t.dimensoes(), (1_000_000, 512));
    }
    #[test]
    fn baseline_documento_preserva_todas_as_celulas_amostradas() {
        let t = preencher(DOC);
        let dense = densa_pequena(DOC).unwrap();
        assert_eq!(dense.len(), 25_600);
        for i in 0..DOC.ocupadas {
            let n = indice(i, DOC.total());
            assert_eq!(dense[n].as_ref(), t.obter(n / DOC.colunas, n % DOC.colunas));
        }
    }
    #[test]
    fn csv_real_apenas_para_documentos_pequenos() {
        let m = medir(DOC);
        assert!(m.csv_bytes.unwrap() > 0);
        assert!(m.ms_csv.is_some());
        assert!((m.alocacao_densa_bytes.unwrap() as u128) >= m.estimativa_densa_bytes);
        assert!(m.ms_consulta_densa.is_some());
    }
    #[test]
    fn payload_csv_pequeno_tem_dimensoes_e_ocupacao_equivalentes() {
        let registro = Registro::padrao();
        let bytes = registro
            .gravar("csv", &Documento::Tabela(preencher(DOC)))
            .unwrap();
        let Documento::Tabela(relida) = registro.ler("csv", &bytes).unwrap() else {
            panic!("esperava uma tabela importada");
        };
        assert_eq!(relida.dimensoes(), (DOC.linhas, DOC.colunas));
        assert_eq!(relida.iter().count(), DOC.ocupadas);
        // CSV é texto: o tipo numérico não é preservado pelo codec demonstrativo.
        let n = indice(0, DOC.total());
        assert_eq!(
            relida
                .obter(n / DOC.colunas, n % DOC.colunas)
                .unwrap()
                .valor,
            Valor::Texto("1".into())
        );
    }
    #[test]
    fn ausencia_de_serializacao_gigante_e_invariante() {
        let m = medir(MASSIVO);
        assert!(m.csv_bytes.is_none());
        assert!(m.ms_csv.is_none());
        assert!(m.alocacao_densa_bytes.is_none());
        assert_eq!(
            m.estimativa_densa_bytes,
            512_000_000u128 * (size_of::<Option<Celula>>() as u128)
        );
    }
}
