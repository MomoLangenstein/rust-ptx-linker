use std::path::{Path, PathBuf};

use anyhow::Context;
use tracing::info;

use crate::{Optimization, Target};

#[derive(Debug)]
pub struct Session {
    target: Target,
    cpu: Option<String>,
    symbols: Vec<String>,
    bitcode: Vec<PathBuf>,

    version: String,

    // Output files
    link_path: PathBuf,
    opt_path: PathBuf,
    sym_path: PathBuf,
    out_path: PathBuf,
}

impl Session {
    pub fn new(
        target: crate::Target,
        cpu: Option<String>,
        out_path: PathBuf,
    ) -> anyhow::Result<Self> {
        let link_path = out_path.with_extension("o");
        let opt_path = out_path.with_extension("optimized.o");
        let sym_path = out_path.with_extension("symbols.txt");

        let version_output = std::process::Command::new("rustc")
            .arg("--version")
            .arg("--verbose")
            .output()?;
        let version_output = String::from_utf8(version_output.stdout).unwrap();

        let mut llvm_version = None;

        for line in version_output.lines() {
            if let Some(version) = str::strip_prefix(line, "LLVM version: ") {
                if let Some((version, _)) = version.split_once('.') {
                    llvm_version = Some(String::from(version));
                    break;
                }
            }
        }

        let Some(llvm_version) = llvm_version else {
            anyhow::bail!("unable to determine LLVM version from:\n{version_output}");
        };

        let version = if let Ok(version_output) = std::process::Command::new(format!("llvm-link-{llvm_version}"))
            .arg("--version")
            .output()
        {
            tracing::info!(
                "using specific llvm-link-{llvm_version} with version:\n{}",
                String::from_utf8(version_output.stdout).unwrap(),
            );

            format!("-{llvm_version}")
        } else if let Ok(version_output) = std::process::Command::new("llvm-link")
            .arg("--version")
            .output()
        {
            tracing::info!(
                "using default llvm-link with version:\n{}",
                String::from_utf8(version_output.stdout).unwrap(),
            );

            String::new()
        } else {
            anyhow::bail!("unable to determine find either llvm-link-{llvm_version} or llvm-link");
        };

        Ok(Session {
            target,
            cpu,
            symbols: Vec::new(),
            bitcode: Vec::new(),
            version,
            link_path,
            opt_path,
            sym_path,
            out_path,
        })
    }

    /// Link a rlib into a bitcode object and add it to the list of files ready
    /// to be linked
    pub fn link_rlib(&mut self, path: impl AsRef<Path>, keep_symbols: bool) -> anyhow::Result<()> {
        let output_file_link = path.as_ref().with_extension("o");
        tracing::info!(
            "Linking rlib: {} into bitcode: {}",
            path.as_ref().display(),
            output_file_link.display(),
        );

        let link_output = std::process::Command::new(format!("llvm-link{}", self.version))
            .arg(path.as_ref())
            .arg("-o")
            .arg(&output_file_link)
            .arg("--ignore-non-bitcode")
            .output()
            .unwrap();

        if !link_output.status.success() {
            tracing::error!(
                "llvm-link returned with Exit status: {}\n stdout: {}\n stderr: {}",
                link_output.status,
                String::from_utf8(link_output.stdout).unwrap(),
                String::from_utf8(link_output.stderr).unwrap(),
            );
            anyhow::bail!("llvm-link failed to link file {}", path.as_ref().display());
        }

        self.add_bitcode(output_file_link, keep_symbols)
    }

    /// Add a bitcode module ready to be linked
    pub fn add_bitcode(
        &mut self,
        path: impl AsRef<Path>,
        keep_symbols: bool,
    ) -> anyhow::Result<()> {
        if keep_symbols {
            let nm_output = std::process::Command::new(format!("llvm-nm{}", self.version))
                .arg("--extern-only")
                .arg("--export-symbols")
                .arg(path.as_ref())
                .output()
                .unwrap();

            if !nm_output.status.success() {
                tracing::error!(
                    "llvm-nm returned with Exit status: {}\n stdout: {}\n stderr: {}",
                    nm_output.status,
                    String::from_utf8(nm_output.stdout).unwrap(),
                    String::from_utf8(nm_output.stderr).unwrap(),
                );
                anyhow::bail!(
                    "llvm-nm failed to return symbols from file {}",
                    path.as_ref().display()
                );
            }

            let symbol_string = String::from_utf8(nm_output.stdout).unwrap();
            let symbols = symbol_string
                .split_whitespace()
                .filter(|s| {
                    *s != "__rg_oom" && *s != "rust_begin_unwind" && !s.starts_with("__rust_")
                })
                .map(String::from)
                .collect::<Vec<_>>();
            info!(
                "Extracted {} symbols from {:?}: {:?}",
                symbols.len(),
                path.as_ref(),
                symbols
            );
            self.symbols.extend(symbols);
        }

        self.bitcode.push(path.as_ref().to_owned());
        Ok(())
    }

    fn link(&mut self) -> anyhow::Result<()> {
        tracing::info!(
            "Linking {} bitcode files using llvm-link",
            self.bitcode.len()
        );

        let llvm_link_output = std::process::Command::new(format!("llvm-link{}", self.version))
            .args(&self.bitcode)
            .arg("-o")
            .arg(&self.link_path)
            .output()
            .unwrap();

        if !llvm_link_output.status.success() {
            tracing::error!(
                "llvm-link returned with Exit status: {}\n stdout: {}\n stderr: {}",
                llvm_link_output.status,
                String::from_utf8(llvm_link_output.stdout).unwrap(),
                String::from_utf8(llvm_link_output.stderr).unwrap(),
            );
            anyhow::bail!("llvm-link failed to link bitcode files {:?}", self.bitcode);
        }

        Ok(())
    }

    /// Optimize using `opt`
    ///
    /// Before this can be called `link` needs to be called
    fn optimize(
        &mut self,
        optimization: Optimization,
        mut internalize: bool,
        mut debug: bool,
        mut inline: bool,
    ) -> anyhow::Result<()> {
        let mut passes = format!("default<{optimization}>");

        // FIXME(@kjetilkjeka) The whole corelib currently cannot be compiled for
        // nvptx64 so everything relies on not using the troublesome symbols and
        // removing them during linking
        if !internalize && self.target == crate::Target::Nvptx64NvidiaCuda {
            tracing::warn!("nvptx64 target detected - internalizing symbols");
            internalize = true;
        }

        // FIXME(@kjetilkjeka) Debug symbol generation is broken for nvptx64 so we must
        // remove them even in debug mode
        if debug && self.target == crate::Target::Nvptx64NvidiaCuda {
            tracing::warn!("nvptx64 target detected - stripping debug symbols");
            debug = false;
        }

        if !inline && self.target == crate::Target::Nvptx64NvidiaCuda {
            tracing::warn!("nvptx64 target detected - inlining all symbols");
            inline = true;
        }

        if internalize {
            passes.push_str(",internalize,globaldce");
            let symbol_file_content = self.symbols.iter().fold(String::new(), |s, x| s + x + "\n");
            std::fs::write(&self.sym_path, symbol_file_content).context(format!(
                "Failed to write symbol file: {}",
                self.sym_path.display()
            ))?;
        }

        tracing::info!("optimizing bitcode with passes: {}", passes);
        let mut opt_cmd = std::process::Command::new(format!("opt{}", self.version));
        opt_cmd
            .arg(&self.link_path)
            .arg("-o")
            .arg(&self.opt_path)
            .arg(format!(
                "--internalize-public-api-file={}",
                self.sym_path.display()
            ))
            .arg(format!("--passes={passes}"));

        if !debug {
            opt_cmd.arg("--strip-debug");
        }

        let opt_output = opt_cmd.output().unwrap();

        if !opt_output.status.success() {
            tracing::error!(
                "opt returned with Exit status: {}\n stdout: {}\n stderr: {}",
                opt_output.status,
                String::from_utf8(opt_output.stdout).unwrap(),
                String::from_utf8(opt_output.stderr).unwrap(),
            );
            anyhow::bail!("opt failed optimize bitcode: {}", self.link_path.display());
        };

        if !inline {
            return Ok(());
        }

        let nm_output = std::process::Command::new(format!("llvm-nm{}", self.version))
            .arg("--format=just-symbols")
            .arg("--defined-only")
            .arg(&self.opt_path)
            .output()
            .unwrap();

        if !nm_output.status.success() {
            tracing::error!(
                "llvm-nm returned with Exit status: {}\n stdout: {}\n stderr: {}",
                nm_output.status,
                String::from_utf8(nm_output.stdout).unwrap(),
                String::from_utf8(nm_output.stderr).unwrap(),
            );
            anyhow::bail!(
                "llvm-nm failed to return symbols from file {}",
                self.opt_path.display()
            );
        }
        let symbol_string = String::from_utf8(nm_output.stdout).unwrap();

        let passes =
            format!("default<{optimization}>,forceattrs,always-inline,gvn,globalopt,mem2reg,dse,globalopt");

        tracing::info!("inlining bitcode with passes: {}", passes);
        let mut opt_cmd = std::process::Command::new(format!("opt{}", self.version));
        opt_cmd
            .arg(&self.opt_path)
            .arg("-o")
            .arg(&self.opt_path)
            .arg(format!("--passes={passes}"));

        for symbol in symbol_string.split_whitespace() {
            opt_cmd.arg(format!("--force-attribute={symbol}:alwaysinline"));
        }

        let opt_output = opt_cmd.output().unwrap();

        if !opt_output.status.success() {
            tracing::error!(
                "opt returned with Exit status: {}\n stdout: {}\n stderr: {}",
                opt_output.status,
                String::from_utf8(opt_output.stdout).unwrap(),
                String::from_utf8(opt_output.stderr).unwrap(),
            );
            anyhow::bail!("opt failed inline bitcode: {}", self.opt_path.display());
        };

        Ok(())
    }

    /// Compile to native format using `llc`
    ///
    /// Before this can be called `optimize` needs to be called
    fn compile(&mut self) -> anyhow::Result<()> {
        let mut lcc_command = std::process::Command::new(format!("llc{}", self.version));

        if let Some(mcpu) = &self.cpu {
            lcc_command.arg("--mcpu").arg(mcpu);
        }

        let lcc_output = lcc_command
            .arg(&self.opt_path)
            .arg("-o")
            .arg(&self.out_path)
            .output()
            .unwrap();

        if !lcc_output.status.success() {
            tracing::error!(
                "llc returned with Exit status: {}\n stdout: {}\n stderr: {}",
                lcc_output.status,
                String::from_utf8(lcc_output.stdout).unwrap(),
                String::from_utf8(lcc_output.stderr).unwrap(),
            );

            anyhow::bail!(
                "llc failed to compile {} into {}",
                self.opt_path.display(),
                self.out_path.display()
            );
        }

        Ok(())
    }

    /// Links, optimizes and compiles to the native format
    pub fn lto(
        &mut self,
        optimization: crate::Optimization,
        internalize: bool,
        debug: bool,
        inline: bool,
    ) -> anyhow::Result<()> {
        self.link()?;
        self.optimize(optimization, internalize, debug, inline)?;
        self.compile()
    }
}
