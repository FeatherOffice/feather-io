//! IMG-010: gera PNGs para inspecionar pipeline, desfazer e ramo novo.
use feather_image::{HistoricoImagem, ImagemRgba, OperacaoImagem};
use std::{env, error::Error, fs, path::Path};

fn salvar_png(imagem: &ImagemRgba, caminho: &Path) -> Result<(), Box<dyn Error>> {
    let (largura, altura) = imagem.dimensoes();
    let buffer = image::RgbaImage::from_raw(largura, altura, imagem.pixels().to_vec())
        .ok_or("buffer RGBA inválido")?;
    buffer.save(caminho)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let pasta = env::args().nth(1).unwrap_or_else(|| "/tmp/featheroffice-img010".into());
    fs::create_dir_all(&pasta)?;
    let largura = 320_u32;
    let altura = 200_u32;
    let mut pixels = Vec::with_capacity(largura as usize * altura as usize * 4);
    for y in 0..altura {
        for x in 0..largura {
            pixels.extend_from_slice(&[
                (x * 255 / largura) as u8,
                (y * 255 / altura) as u8,
                (((x / 20 + y / 20) % 2) * 220) as u8,
                if x < largura / 4 { 128 } else { 255 },
            ]);
        }
    }

    let origem = ImagemRgba::nova(largura, altura, pixels).expect("imagem valida");
    let mut editor = HistoricoImagem::com_limite_cache(origem, 2 * 1024 * 1024);
    salvar_png(editor.origem(), Path::new(&pasta).join("00_original.png").as_path())?;

    editor.aplicar(OperacaoImagem::TonsDeCinza).expect("cinza");
    editor.aplicar(OperacaoImagem::EspelharHorizontal).expect("espelhamento");
    let editada = editor.resultado().expect("render");
    salvar_png(&editada, Path::new(&pasta).join("01_editada.png").as_path())?;

    let sem_espelho = editor.desfazer().expect("undo").expect("estado");
    salvar_png(&sem_espelho, Path::new(&pasta).join("02_undo.png").as_path())?;

    let refeita = editor.refazer().expect("redo").expect("estado");
    assert_eq!(editada, refeita);
    salvar_png(&refeita, Path::new(&pasta).join("03_redo.png").as_path())?;

    editor.desfazer().expect("undo");
    let ramo = editor.aplicar(OperacaoImagem::Opacidade { fator: 0.4 })
        .expect("ramificacao");
    assert!(!editor.pode_refazer());
    assert_eq!(editor.pipeline_atual().executar().expect("pipeline imutavel"), *ramo);
    salvar_png(&ramo, Path::new(&pasta).join("04_novo_ramo.png").as_path())?;

    println!("IMG-010 OK: 5 PNGs em {pasta}");
    println!("Etapas: {} | cursor: {} | bytes de cache: {}",
        editor.operacoes().len(), editor.indice(), editor.bytes_cache());
    Ok(())
}
