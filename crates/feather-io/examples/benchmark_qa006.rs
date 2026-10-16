//! QA-006: benchmarks repetíveis (sem dependências externas nem GUI).
//!
//! Execute em release, fora da suíte de testes comuns:
//! cargo run --release -p feather-io \
//!     --example benchmark_qa006 -- --full --iteracoes 5 --aquecimento 2 \
//!     --csv /tmp/featheroffice-qa006.csv
//!
//! As cargas são determinísticas; os tempos são observações da máquina,
//! NÃO valores de referência fixos ou gates arbitrários de desempenho.
use feather_charts::{Grafico, Serie, Tipo};
use feather_draw::Cor;
use feather_image::ImagemRgba;
use feather_tables::{Celula, Tabela, Valor};
use std::fmt::Write as _;
use std::hint::black_box;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Perfil {
    Rapido,
    Completo,
}

impl Perfil {
    fn nome(self) -> &'static str {
        match self {
            Self::Rapido => "rapido",
            Self::Completo => "completo",
        }
    }

    /// (categorias, séries, largura, altura, linhas densas, colunas densas,
    ///  linhas esparsas, colunas esparsas, células esparsas)
    fn tamanhos(self) -> (usize, usize, u32, u32, usize, usize, usize, usize, usize) {
        match self {
            Self::Rapido => (120, 4, 256, 192, 24, 20, 10_000, 600, 150),
            Self::Completo => (
                2_000, 32, 3_840, 2_160, 400, 240, 1_000_000, 2_048, 20_000,
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Configuracao {
    perfil: Perfil,
    iteracoes: usize,
    aquecimento: usize,
    csv: Option<String>,
}

fn argumentos<I: IntoIterator<Item = String>>(args: I) -> Result<Configuracao, String> {
    let mut config = Configuracao {
        perfil: Perfil::Completo,
        iteracoes: 5,
        aquecimento: 1,
        csv: None,
    };
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--quick" => config.perfil = Perfil::Rapido,
            "--full" => config.perfil = Perfil::Completo,
            "--iteracoes" => {
                let valor = iter.next().ok_or("--iteracoes requer um número")?;
                config.iteracoes = valor.parse().map_err(|_| "--iteracoes inválido")?;
                if config.iteracoes == 0 || config.iteracoes > 100 {
                    return Err("--iteracoes deve estar entre 1 e 100".into());
                }
            }
            "--aquecimento" => {
                let valor = iter.next().ok_or("--aquecimento requer um número")?;
                config.aquecimento = valor.parse().map_err(|_| "--aquecimento inválido")?;
                if config.aquecimento > 20 {
                    return Err("--aquecimento deve ser até 20".into());
                }
            }
            "--csv" => {
                let arquivo = iter.next().ok_or("--csv requer um caminho")?;
                if arquivo.is_empty() {
                    return Err("--csv requer um caminho não vazio".into());
                }
                config.csv = Some(arquivo);
            }
            "--help" | "-h" => {
                return Err("Uso: benchmark_qa006 [--quick|--full] [--iteracoes 1..100] [--aquecimento 0..20] [--csv arquivo.csv]".into());
            }
            _ => return Err(format!("argumento não reconhecido: {arg}")),
        }
    }
    Ok(config)
}

fn grafico_deterministico(quantidade_categorias: usize, quantidade_series: usize) -> Grafico {
    let categorias = (0..quantidade_categorias)
        .map(|i| format!("C{i:04}"))
        .collect();
    let series = (0..quantidade_series)
        .map(|s| Serie {
            nome: format!("S{s:02}"),
            // Inclui valores positivos e negativos, sem depender de RNG externo.
            valores: (0..quantidade_categorias)
                .map(|i| ((i * 37 + s * 71) % 211) as f64 - 105.0)
                .collect(),
            cor: Cor::rgb(
                (s as u8).wrapping_mul(47),
                (s as u8).wrapping_mul(61),
                (s as u8).wrapping_mul(19),
            ),
        })
        .collect();
    Grafico {
        titulo: "Benchmark de séries".into(),
        tipo: Tipo::Linhas,
        categorias,
        series,
    }
}

fn imagem_deterministica(largura: u32, altura: u32) -> ImagemRgba {
    let pixels = (largura as usize)
        .checked_mul(altura as usize)
        .and_then(|n| n.checked_mul(4))
        .expect("dimensões do benchmark dentro de usize");
    let mut buffer = vec![0_u8; pixels];
    for (i, pixel) in buffer.chunks_exact_mut(4).enumerate() {
        pixel.copy_from_slice(&[
            i as u8,
            (i as u8).wrapping_mul(3),
            (i as u8).wrapping_add(19),
            (i % 256) as u8,
        ]);
    }
    ImagemRgba::nova(largura, altura, buffer).expect("fixture RGBA válida")
}

fn tabela_densa(linhas: usize, colunas: usize) -> Tabela {
    let mut tabela = Tabela::nova(linhas, colunas);
    for l in 0..linhas {
        for c in 0..colunas {
            tabela
                .definir(
                    l,
                    c,
                    Celula {
                        valor: Valor::Numero(((l * 29 + c * 7) % 1_009) as f64),
                        ..Default::default()
                    },
                )
                .expect("índice denso válido");
        }
    }
    tabela
}

fn tabela_esparsa(linhas: usize, colunas: usize, ocupadas: usize) -> Tabela {
    let mut tabela = Tabela::nova(linhas, colunas);
    for i in 0..ocupadas {
        // As duas progressões têm períodos maiores que ocupadas neste perfil.
        let l = (i * 137) % linhas;
        let c = (i * 61) % colunas;
        tabela
            .definir(
                l,
                c,
                Celula {
                    valor: Valor::Numero((i + 1) as f64),
                    ..Default::default()
                },
            )
            .expect("índice esparso válido");
    }
    tabela
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Amostra {
    nanos: u128,
    verificacao: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Medicao {
    cenario: &'static str,
    amostras: Vec<Amostra>,
}

fn medir<F: FnMut() -> usize>(
    cenario: &'static str,
    configuracao: &Configuracao,
    mut operacao: F,
) -> Medicao {
    for _ in 0..configuracao.aquecimento {
        black_box(operacao());
    }
    let mut amostras = Vec::with_capacity(configuracao.iteracoes);
    for _ in 0..configuracao.iteracoes {
        let inicio = Instant::now();
        let verificacao = black_box(operacao());
        amostras.push(Amostra {
            nanos: inicio.elapsed().as_nanos(),
            verificacao,
        });
    }
    Medicao { cenario, amostras }
}

fn percentil_ns(amostras: &[Amostra], percentil: usize) -> u128 {
    let mut tempos: Vec<u128> = amostras.iter().map(|v| v.nanos).collect();
    tempos.sort_unstable();
    let indice = ((tempos.len() * percentil).div_ceil(100)).saturating_sub(1);
    tempos[indice]
}

fn csv_resultados(configuracao: &Configuracao, resultados: &[Medicao]) -> String {
    let mut csv = String::from("perfil,cenario,iteracao,duracao_ns,verificacao\n");
    for resultado in resultados {
        for (i, amostra) in resultado.amostras.iter().enumerate() {
            writeln!(
                csv,
                "{},{},{},{},{}",
                configuracao.perfil.nome(),
                resultado.cenario,
                i + 1,
                amostra.nanos,
                amostra.verificacao
            )
            .expect("gravação em String não falha");
        }
    }
    csv
}

fn executar(config: &Configuracao) -> Vec<Medicao> {
    let (categorias, series, largura, altura, densas_linhas, densas_colunas,
         esparsas_linhas, esparsas_colunas, ocupadas) = config.perfil.tamanhos();

    // Preparação de gráficos e imagem FORA da janela cronometrada.
    let grafico = grafico_deterministico(categorias, series);
    let imagem = imagem_deterministica(largura, altura);
    let mut resultados = Vec::with_capacity(5);

    resultados.push(medir("grafico_series_cena", config, || {
        let cena = black_box(grafico.cena(1_200.0, 720.0).expect("dados de gráfico válidos"));
        cena.figuras.len()
    }));
    resultados.push(medir("grafico_series_lttb", config, || {
        let cena = black_box(
            grafico.cena_amostrada(1_200.0, 720.0, 100)
                .expect("limite de amostragem válido"),
        );
        cena.figuras.len()
    }));
    resultados.push(medir("imagem_rgba_filtros", config, || {
        let mut copia = imagem.clone();
        copia.ajustar_luz(0.2, -0.1);
        copia.tons_de_cinza();
        let (w, h) = copia.dimensoes();
        let reduzida = black_box(
            copia.redimensionar_vizinho(w / 2, h / 2)
                .expect("dimensões não nulas"),
        );
        reduzida.pixels().len()
    }));
    resultados.push(medir("tabela_densa_construcao", config, || {
        let tabela = black_box(tabela_densa(densas_linhas, densas_colunas));
        tabela.iter().count()
    }));
    resultados.push(medir("tabela_esparsa_construcao", config, || {
        let tabela = black_box(tabela_esparsa(esparsas_linhas, esparsas_colunas, ocupadas));
        tabela.iter().count()
    }));
    resultados
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = argumentos(std::env::args().skip(1))
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    println!(
        "QA-006 perfil={} repeticoes={} aquecimento={} sistema={}-{} paralelismo={}",
        config.perfil.nome(),
        config.iteracoes,
        config.aquecimento,
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::thread::available_parallelism().map_or(1, |x| x.get())
    );
    let resultados = executar(&config);
    for medicao in &resultados {
        println!(
            "{:<32} p50={:>10.3} ms p95={:>10.3} ms verificação={}",
            medicao.cenario,
            percentil_ns(&medicao.amostras, 50) as f64 / 1_000_000.0,
            percentil_ns(&medicao.amostras, 95) as f64 / 1_000_000.0,
            medicao.amostras[0].verificacao
        );
        assert!(
            medicao.amostras.iter().all(|x| x.verificacao == medicao.amostras[0].verificacao),
            "resultado instável: {}",
            medicao.cenario
        );
    }
    if let Some(caminho) = &config.csv {
        std::fs::write(caminho, csv_resultados(&config, &resultados))?;
        println!("Amostras individuais em {caminho}");
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn argumentos_rejeitam_repeticoes_invalidas() {
        let config = argumentos(["--quick".to_owned(), "--iteracoes".into(), "3".into()])
            .unwrap();
        assert_eq!(config.perfil, Perfil::Rapido);
        assert_eq!(config.iteracoes, 3);
        assert!(argumentos(["--iteracoes".to_owned(), "0".into()]).is_err());
        assert!(argumentos(["--aquecimento".to_owned(), "21".into()]).is_err());
        assert!(argumentos(["--csv".to_owned()]).is_err());
    }

    #[test]
    fn carga_esparsa_nao_cria_grade_densa() {
        let t = tabela_esparsa(1_000_000, 2_048, 20_000);
        assert_eq!(t.dimensoes(), (1_000_000, 2_048));
        assert_eq!(t.iter().count(), 20_000);
    }

    #[test]
    fn cargas_sao_deterministicas_e_validas() {
        let a = grafico_deterministico(120, 4);
        assert_eq!(a, grafico_deterministico(120, 4));
        assert!(a.validar().is_ok());
        assert_eq!(imagem_deterministica(3, 2), imagem_deterministica(3, 2));
        assert_eq!(tabela_densa(3, 5).iter().count(), 15);
    }

    #[test]
    fn smoke_rapido_executa_graficos_imagens_e_tabelas_reais() {
        let cfg = Configuracao {
            perfil: Perfil::Rapido,
            iteracoes: 1,
            aquecimento: 0,
            csv: None,
        };
        let resultados = executar(&cfg);
        assert_eq!(resultados.len(), 5);
        assert!(resultados[0].amostras[0].verificacao > 0);
        assert!(resultados[1].amostras[0].verificacao > 0);
        // Imagem 256x192 reduzida a 128x96 RGBA.
        assert_eq!(resultados[2].amostras[0].verificacao, 128 * 96 * 4);
        assert_eq!(resultados[3].amostras[0].verificacao, 24 * 20);
        assert_eq!(resultados[4].amostras[0].verificacao, 150);
        assert_eq!(csv_resultados(&cfg, &resultados).lines().count(), 6);
    }

    #[test]
    fn cronometro_e_csv_retornam_todas_as_amostras() {
        let cfg = Configuracao {
            perfil: Perfil::Rapido,
            iteracoes: 3,
            aquecimento: 1,
            csv: None,
        };
        let mut chamadas = 0;
        let medicao = medir("teste", &cfg, || {
            chamadas += 1;
            7
        });
        assert_eq!(chamadas, 4);
        assert_eq!(medicao.amostras.len(), 3);
        let mut duracoes: Vec<_> = medicao.amostras.iter().map(|a| a.nanos).collect();
        duracoes.sort_unstable();
        assert_eq!(percentil_ns(&medicao.amostras, 50), duracoes[1]);
        let csv = csv_resultados(&cfg, &[medicao]);
        assert_eq!(csv.lines().count(), 4);
        assert!(csv.contains("rapido,teste,3,"));
    }
}
