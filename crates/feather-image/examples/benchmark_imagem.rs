//! Benchmark reproduzível de buffers RGBA grandes, sem dependências de benchmark.
//!
//! Exemplo:
//! cargo run --release -p feather-image \
//!     --example benchmark_imagem -- 4096 4096 5
//!
//! Saída em TSV: tempo por operação e RSS/pico de RSS do processo (Linux).
//! Em outras plataformas, memória aparece como N/D. A alocação dos buffers
//! de saída de redimensionamento é necessária, não uma cópia do buffer fonte.

use feather_image::ImagemRgba;
use std::hint::black_box;
use std::time::Instant;

fn parametro(texto: Option<String>, padrao: u32, nome: &str) -> Result<u32, String> {
    match texto {
        Some(texto) => texto
            .parse::<u32>()
            .map_err(|_| format!("{nome} deve ser inteiro positivo")),
        None => Ok(padrao),
    }
}

fn memoria_kib(chave: &str) -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|linha| {
        linha
            .strip_prefix(chave)?
            .split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()
    })
}

fn memoria_formatada(chave: &str) -> String {
    memoria_kib(chave).map_or_else(|| "N/D".into(), |n| n.to_string())
}

fn medir<F>(operacao: &str, repeticoes: u32, mut tarefa: F) -> Result<(), String>
where
    F: FnMut() -> Result<(), String>,
{
    let mut tempos = Vec::with_capacity(repeticoes as usize);
    for _ in 0..repeticoes {
        let inicio = Instant::now();
        tarefa()?;
        tempos.push(inicio.elapsed().as_secs_f64() * 1000.0);
    }
    let media = tempos.iter().sum::<f64>() / f64::from(repeticoes);
    let minimo = tempos.iter().copied().fold(f64::INFINITY, f64::min);
    let maximo = tempos.iter().copied().fold(0.0, f64::max);
    println!(
        "{operacao}\t{media:.3}\t{minimo:.3}\t{maximo:.3}\t{}\t{}",
        memoria_formatada("VmRSS:"),
        memoria_formatada("VmHWM:")
    );
    Ok(())
}

fn executar() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let largura = parametro(args.next(), 4096, "largura")?;
    let altura = parametro(args.next(), 4096, "altura")?;
    let repeticoes = parametro(args.next(), 5, "repeticoes")?;
    if args.next().is_some() {
        return Err("use: benchmark_imagem [largura altura repeticoes]".into());
    }
    if largura == 0 || altura == 0 || repeticoes == 0 || repeticoes > 100 {
        return Err("largura, altura e repeticoes devem ser > 0 (maximo 100 repeticoes)".into());
    }
    let bytes = (largura as usize)
        .checked_mul(altura as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or("dimensoes excedem capacidade de endereco")?;
    if bytes > 512 * 1024 * 1024 {
        return Err("limite de seguranca: maximo 512 MiB de pixels na entrada".into());
    }
    let mut pixels = vec![0u8; bytes];
    for (i, pixel) in pixels.chunks_exact_mut(4).enumerate() {
        let v = (i as u8).wrapping_mul(17);
        pixel.copy_from_slice(&[v, v.wrapping_add(63), v / 2, i as u8]);
    }
    let imagem = ImagemRgba::nova(largura, altura, pixels)
        .map_err(|e| format!("imagem inicial invalida: {e:?}"))?;
    let mut origem = Some(imagem);

    println!("# dimensoes={largura}x{altura} pixels_rgba_bytes={bytes} repeticoes={repeticoes}");
    println!(
        "# VmRSS e VmHWM incluem toda a memoria do processo; VmHWM e pico cumulativo Linux (KiB)"
    );
    println!("# conversao_propriedade e visao_emprestada nao criam buffers de pixels adicionais");
    println!("operacao\tmedia_ms\tmin_ms\tmax_ms\trss_kib\tpico_rss_kib");

    medir("conversao_propriedade", repeticoes, || {
        let interna = origem.take().expect("origem mantida entre repeticoes");
        let ptr = interna.pixels().as_ptr();
        let externa = interna.em_image();
        if externa.as_raw().as_ptr() != ptr {
            return Err("conversao de saida copiou pixels".into());
        }
        let interna = ImagemRgba::de_image(externa)
            .map_err(|e| format!("conversao de entrada falhou: {e:?}"))?;
        if interna.pixels().as_ptr() != ptr {
            return Err("conversao de entrada copiou pixels".into());
        }
        origem = Some(interna);
        Ok(())
    })?;

    medir("visao_emprestada", repeticoes, || {
        let interna = origem.as_ref().expect("imagem existente");
        let visao = interna.como_image();
        if visao.as_raw().as_ptr() != interna.pixels().as_ptr() {
            return Err("visao emprestada copiou pixels".into());
        }
        black_box(visao.get_pixel(0, 0));
        Ok(())
    })?;

    medir("espelhar_horizontal", repeticoes, || {
        origem
            .as_mut()
            .expect("imagem existente")
            .espelhar_horizontal();
        black_box(origem.as_ref().expect("imagem existente").pixels()[0]);
        Ok(())
    })?;

    medir("tons_de_cinza", repeticoes, || {
        origem.as_mut().expect("imagem existente").tons_de_cinza();
        black_box(origem.as_ref().expect("imagem existente").pixels()[0]);
        Ok(())
    })?;

    medir("redimensionar_metade", repeticoes, || {
        let interna = origem.as_ref().expect("imagem existente");
        let reduzida = interna
            .redimensionar_vizinho((largura / 2).max(1), (altura / 2).max(1))
            .map_err(|e| format!("redimensionamento falhou: {e:?}"))?;
        black_box(reduzida.pixels()[0]);
        Ok(())
    })?;

    Ok(())
}

fn main() {
    if let Err(erro) = executar() {
        eprintln!("benchmark_imagem: {erro}");
        std::process::exit(1);
    }
}
