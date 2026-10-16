//! Efeitos de acesso rápido compartilhados entre FeatherDocs e FeatherSheet.
//!
//! Os consumidores não reimplementam blur, realce ou conversão para cinza;
//! apenas selecionam o efeito por chave estável para a interface Iced.
//! A operação é não destrutiva: processa a partir de `ImagemRgba` emprestada.
use crate::{ImagemRgba, OperacaoImagem, ErroPipeline};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EfeitoRapido {
    Original,
    Cinza,
    Desfoque,
    Nitidez,
}
impl EfeitoRapido {
    pub const TODOS: [Self; 4] = [Self::Original, Self::Cinza, Self::Desfoque, Self::Nitidez];

    pub const fn chave(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Cinza => "cinza",
            Self::Desfoque => "desfoque",
            Self::Nitidez => "nitidez",
        }
    }
    pub fn da_chave(chave: &str) -> Option<Self> {
        Self::TODOS.into_iter().find(|efeito| efeito.chave() == chave)
    }
    /// Mesmo algoritmo, parâmetros e preservação alfa para Docs e Sheet.
    pub fn executar(self, origem: &ImagemRgba) -> Result<ImagemRgba, ErroPipeline> {
        let op = match self {
            Self::Original => return Ok(origem.clone()),
            Self::Cinza => OperacaoImagem::TonsDeCinza,
            Self::Desfoque => OperacaoImagem::Desfocar { raio: 2 },
            Self::Nitidez => OperacaoImagem::Nitidez { intensidade: 0.4 },
        };
        op.executar(origem)
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    #[test]
    fn chaves_estaveis_e_processamento_sem_mutar_fonte() {
        let original = ImagemRgba::nova(3, 1, vec![
            255, 0, 0, 0, 0, 255, 0, 128, 0, 0, 255, 255,
        ]).unwrap();
        let copia = original.clone();
        for efeito in EfeitoRapido::TODOS {
            assert_eq!(EfeitoRapido::da_chave(efeito.chave()), Some(efeito));
            let result = efeito.executar(&original).unwrap();
            assert_eq!(result.dimensoes(), original.dimensoes());
            for (origem, resultado) in original.pixels().chunks_exact(4)
                .zip(result.pixels().chunks_exact(4)) {
                assert_eq!(origem[3], resultado[3], "alfa modificado por {:?}", efeito);
            }
        }
        assert_eq!(original, copia);
        assert!(EfeitoRapido::da_chave("desconhecido").is_none());
        assert_eq!(EfeitoRapido::Original.executar(&original).unwrap(), copia);
    }
}
