//! QA-008: testes de propriedades reprodutíveis, sem dependências de fuzz externas.
//! Para reproduzir uma falha, mantenha a semente e a iteração exibidas pelo assert.

use feather_draw::{Ponto, Retangulo, Transformacao};
use feather_image::{ErroImagem, ImagemRgba};
use feather_io::{Documento, Registro};
use feather_tables::{Celula, Regiao, Tabela, Valor};

/// xorshift64: sequência determinística independente de relógio e do SO.
struct Gerador(u64);
impl Gerador {
    fn proximo(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn intervalo(&mut self, limite: usize) -> usize {
        (self.proximo() % limite as u64) as usize
    }
}

#[test]
fn csv_roundtrip_unicode_aspas_vazios_crlf_e_virgulas() {
    const CASOS: [&str; 10] = [
        "", "texto", "área", "um, dois", "aspas \"internas\"",
        "linha 1\nlinha 2", "CR\rLF", "linha\r\nnova", "東京", "😀",
    ];
    const SEMENTE: u64 = 0x43_53_56_21_20_26_00_01;
    let mut rng = Gerador(SEMENTE);
    let registro = Registro::padrao();
    for caso in 0..256 {
        let linhas = 1 + rng.intervalo(8);
        let colunas = 1 + rng.intervalo(8);
        let mut tabela = Tabela::nova(linhas, colunas);
        for l in 0..linhas {
            for c in 0..colunas {
                let valor = CASOS[rng.intervalo(CASOS.len())];
                if !valor.is_empty() {
                    tabela.definir(l, c, Celula {
                        valor: Valor::Texto(valor.to_string()),
                        ..Default::default()
                    }).unwrap();
                }
            }
        }
        // O codec CSV não preserva uma tabela totalmente vazia 1x1.
        // Uma âncora na última linha distingue uma linha vazia de EOF.
        tabela.definir(linhas - 1, 0, Celula {
            valor: Valor::Texto("âncora".into()),
            ..Default::default()
        }).unwrap();
        let original = Documento::Tabela(tabela);
        let bytes = registro.gravar("csv", &original).unwrap();
        let decodificado = registro.ler("csv", &bytes).unwrap();
        assert_eq!(decodificado, original, "seed={SEMENTE:#x}, caso={caso}");
        assert_eq!(
            registro.gravar("csv", &decodificado).unwrap(),
            bytes,
            "serialização instável: seed={SEMENTE:#x}, caso={caso}"
        );
    }
}

#[test]
fn geometria_composicao_afim_e_inclusao_retangulos() {
    const SEMENTE: u64 = 0x47_45_4f_4d_20_26_00_01;
    let mut rng = Gerador(SEMENTE);
    for caso in 0..512 {
        let x = rng.intervalo(200) as f32 - 100.0;
        let y = rng.intervalo(200) as f32 - 100.0;
        let escala_x = (rng.intervalo(30) as f32 + 1.0) / 10.0;
        let escala_y = (rng.intervalo(30) as f32 + 1.0) / 10.0;
        let p = Ponto::novo(x, y);
        let translacao = Transformacao::transladar(13.0, -7.0);
        let escala = Transformacao::escalar(escala_x, escala_y);
        let composto = translacao.compor(escala).aplicar(p);
        let sequencial = translacao.aplicar(escala.aplicar(p));
        assert!(
            (composto.x - sequencial.x).abs() < 0.001 &&
            (composto.y - sequencial.y).abs() < 0.001,
            "seed={SEMENTE:#x}, caso={caso}"
        );

        let w = 1.0 + rng.intervalo(40) as f32;
        let h = 1.0 + rng.intervalo(40) as f32;
        let r = Retangulo::novo(x, y, w, h);
        assert!(r.contem(Ponto::novo(x + w * 0.5, y + h * 0.5)));
        assert!(!r.contem(Ponto::novo(x - 1.0, y + h * 0.5)));
    }
}

#[test]
fn rgba_tamanho_alfa_e_espelhamento_involutivo() {
    const SEMENTE: u64 = 0x52_47_42_41_20_26_00_01;
    let mut rng = Gerador(SEMENTE);
    for caso in 0..256 {
        let w = 1 + rng.intervalo(16);
        let h = 1 + rng.intervalo(16);
        let mut pixels = vec![0_u8; w * h * 4];
        for (i, pixel) in pixels.chunks_exact_mut(4).enumerate() {
            pixel[0] = rng.proximo() as u8;
            pixel[1] = rng.proximo() as u8;
            pixel[2] = rng.proximo() as u8;
            pixel[3] = [0, 128, 255][i % 3];
        }
        let original = ImagemRgba::nova(w as u32, h as u32, pixels.clone()).unwrap();
        assert_eq!(
            ImagemRgba::nova(w as u32, h as u32, vec![0; pixels.len() + 1]),
            Err(ErroImagem::PixelsInvalidos)
        );
        let mut imagem = original.clone();
        imagem.espelhar_horizontal();
        imagem.espelhar_horizontal();
        imagem.espelhar_vertical();
        imagem.espelhar_vertical();
        assert_eq!(imagem, original, "seed={SEMENTE:#x}, caso={caso}");
        assert_eq!(imagem.pixels(), pixels);
        assert_eq!(imagem.dimensoes(), (w as u32, h as u32));
    }
    assert_eq!(
        ImagemRgba::nova(0, 1, vec![]),
        Err(ErroImagem::DimensoesInvalidas)
    );
}

#[test]
fn mesclagens_preservam_dados_em_undo_redo_e_recusam_sobreposicao() {
    const SEMENTE: u64 = 0x4d_45_53_43_20_26_00_01;
    let mut rng = Gerador(SEMENTE);
    for caso in 0..256 {
        let l = rng.intervalo(7);
        let c = rng.intervalo(7);
        let linhas = 2 + rng.intervalo(8 - l - 1);
        let colunas = 2 + rng.intervalo(8 - c - 1);
        let regiao = Regiao {
            linha: l, coluna: c, linhas, colunas,
        };
        let mut tabela = Tabela::nova(8, 8);
        tabela.definir(l, c, Celula {
            valor: Valor::Texto(format!("origem-{caso}")),
            ..Default::default()
        }).unwrap();
        let antes = tabela.clone();
        let edicao = tabela.mesclar_com_undo(regiao).unwrap();
        let depois = tabela.clone();
        assert_eq!(tabela.mesclagens(), &[regiao]);
        assert!(tabela.mesclar(regiao).is_err(), "seed={SEMENTE:#x}, caso={caso}");
        assert_eq!(tabela, depois);
        edicao.desfazer(&mut tabela).unwrap();
        assert_eq!(tabela, antes, "seed={SEMENTE:#x}, caso={caso}");
        edicao.refazer(&mut tabela).unwrap();
        assert_eq!(tabela, depois);
        tabela.normalizar_mesclagens().unwrap();
        let desfazer_desmesclagem = tabela.desmesclar_com_undo(l, c).unwrap();
        assert!(tabela.mesclagens().is_empty());
        desfazer_desmesclagem.desfazer(&mut tabela).unwrap();
        assert_eq!(tabela, depois);
    }
}
