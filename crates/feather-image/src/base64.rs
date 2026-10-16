//! Base64 RFC 4648 e `data:` URLs para imagens incorporadas.
//!
//! Implementação única consumida pelos adaptadores FeatherSlide e FeatherDocs.
//! Sem alocação de imagem/pixel, sem Iced e sem conversão de formato nativo.
//! Compatibilidade intencional com o comportamento histórico dos dois codecs:
//! whitespace é ignorado na leitura e padding `=` não altera bytes retornados.

const ALFABETO: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Os bytes em base64, com o `=` de preenchimento.
pub fn codificar(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for bloco in bytes.chunks(3) {
        let n = bloco.iter().enumerate().fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= bloco.len() {
                s.push(ALFABETO[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

fn valor(c: u8) -> Option<u32> {
    ALFABETO.iter().position(|&a| a == c).map(|p| p as u32)
}

/// Os bytes de um texto em base64 (ignora espaço e quebra de linha). `None` se tiver caractere inválido.
pub fn decodificar(texto: &str) -> Option<Vec<u8>> {
    let limpos: Vec<u8> = texto.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=').collect();
    let mut saida = Vec::with_capacity(limpos.len() * 3 / 4);
    for bloco in limpos.chunks(4) {
        if bloco.len() == 1 {
            return None;
        }
        let mut n = 0u32;
        for (i, c) in bloco.iter().enumerate() {
            n |= valor(*c)? << (18 - 6 * i);
        }
        for i in 0..bloco.len() - 1 {
            saida.push((n >> (16 - 8 * i)) as u8);
        }
    }
    Some(saida)
}

/// Uma `data:` URL partida em tipo e bytes (`data:image/png;base64,…`).
pub fn de_data_url(url: &str) -> Option<(String, Vec<u8>)> {
    let resto = url.strip_prefix("data:")?;
    let (cabeca, dados) = resto.split_once(',')?;
    let tipo = cabeca.strip_suffix(";base64")?;
    Some((tipo.to_string(), decodificar(dados)?))
}

/// A `data:` URL de um arquivo.
pub fn para_data_url(tipo: &str, bytes: &[u8]) -> String {
    format!("data:{tipo};base64,{}", codificar(bytes))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ida_e_volta_em_todos_os_tamanhos() {
        for texto in ["", "a", "ab", "abc", "abcd", "Olá, mundo!"] {
            assert_eq!(decodificar(&codificar(texto.as_bytes())).unwrap(), texto.as_bytes());
        }
        assert_eq!(codificar(b"Man"), "TWFu");
        assert_eq!(codificar(b"Ma"), "TWE=");
    }

    #[test]
    fn data_url() {
        let url = para_data_url("image/png", &[1, 2, 3, 250]);
        assert_eq!(de_data_url(&url), Some(("image/png".to_string(), vec![1, 2, 3, 250])));
        assert_eq!(de_data_url("https://x.org/a.png"), None);
        assert_eq!(decodificar("@@@@"), None);
    }
}

#[cfg(test)]
mod regressao_compartilhada {
    use super::*;

    #[test]
    fn byte_a_byte_toda_a_faixa_e_padding() {
        let dados: Vec<u8> = (0..=255).collect();
        assert_eq!(decodificar(&codificar(&dados)), Some(dados));
        assert_eq!(codificar(b""), "");
        assert_eq!(codificar(b"f"), "Zg==");
        assert_eq!(codificar(b"fo"), "Zm8=");
        assert_eq!(codificar(b"foo"), "Zm9v");
        assert_eq!(decodificar("Z m8= \n"), Some(b"fo".to_vec()));
    }

    #[test]
    fn svg_unicode_e_mimes_sao_preservados() {
        let svg = "<svg><text>ação &amp; Ω</text></svg>";
        let url = para_data_url("image/svg+xml", svg.as_bytes());
        assert!(url.starts_with("data:image/svg+xml;base64,"));
        assert_eq!(de_data_url(&url), Some(("image/svg+xml".to_string(), svg.as_bytes().to_vec())));
        assert_eq!(de_data_url("https://exemplo.com/teste.svg"), None);
        assert_eq!(de_data_url("data:image/png,abc"), None);
        assert_eq!(decodificar("@@@@"), None);
    }
}
