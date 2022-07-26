extern crate num_bigint_dig as num_bigint;
extern crate num_traits;
extern crate serde;
extern crate serde_derive;
#[macro_use]
extern crate lalrpop_util;

lalrpop_mod!(pub lang);

mod errors;
mod include_logic;
mod parser_logic;
use include_logic::{FileStack, IncludesGraph};
use program_structure::error_code::ReportCode;
use program_structure::error_definition::{Report, ReportCollection};
use program_structure::file_definition::{FileLibrary};
use program_structure::program_archive::ProgramArchive;
use std::path::PathBuf;
use std::str::FromStr;

pub type Version = (usize, usize, usize);

pub fn run_parser(
    file: String,
    version: &str
) -> Result<(ProgramArchive, ReportCollection), (FileLibrary, ReportCollection)> {
    let mut file_library = FileLibrary::new();
    let mut definitions = Vec::new();
    let mut main_components = Vec::new();
    let mut file_stack = FileStack::new(PathBuf::from(file));
    let mut includes_graph = IncludesGraph::new();
    let mut warnings = Vec::new();

    while let Some(crr_file) = FileStack::take_next(&mut file_stack) {
        let (path, src) = open_file(crr_file.clone())
            .map_err(|e| (file_library.clone(), vec![e]))?;
        let file_id = file_library.add_file(path.clone(), src.clone());
        let program = parser_logic::parse_file(&src, file_id)
            .map_err(|e| (file_library.clone(), vec![e]))?;
        if let Some(main) = program.main_component {
            main_components.push((file_id, main));
        }
        includes_graph.add_node(crr_file, program.custom_gates, program.custom_gates_declared);
        let includes = program.includes;
        definitions.push((file_id, program.definitions));
        for include in includes {
            FileStack::add_include(&mut file_stack, include.clone())
                .map_err(|e| (file_library.clone(), vec![e]))?;
            includes_graph.add_edge(include).map_err(|e| (file_library.clone(), vec![e]))?;
        }
        warnings.append(
            &mut check_number_version(
                path,
                program.compiler_version,
                parse_number_version(version)
            ).map_err(|e| (file_library.clone(), vec![e]))?
        );
    }

    if main_components.len() == 0 {
        let report = errors::NoMainError::produce_report();
        Err((file_library, vec![report]))
    } else if main_components.len() > 1 {
        let report = errors::MultipleMainError::produce_report();
        Err((file_library, vec![report]))
    } else {
        let errors: ReportCollection = includes_graph.get_problematic_paths().iter().map(|path|
            Report::error(
                format!(
                    "Missing custom templates pragma in the following chain of includes {}",
                    IncludesGraph::display_path(path)
                ),
                ReportCode::CustomGatesPragmaError
            )
        ).collect();
        if errors.len() > 0 {
            Err((file_library, errors))
        } else {
            let (main_id, main_component) = main_components.pop().unwrap();
            let result_program_archive =
                ProgramArchive::new(file_library, main_id, main_component, definitions);
            match result_program_archive {
                Err((lib, rep)) => {
                    Err((lib, rep))
                }
                Ok(program_archive) => {
                    Ok((program_archive, warnings))
                }
            }
        }
    }
}

fn open_file(path: PathBuf) -> Result<(String, String), Report> /* path, src */ {
    use errors::FileOsError;
    use std::fs::read_to_string;
    let path_str = format!("{:?}", path);
    read_to_string(path)
        .map(|contents| (path_str.clone(), contents))
        .map_err(|_| FileOsError { path: path_str.clone() })
        .map_err(|e| FileOsError::produce_report(e))
}

fn parse_number_version(version: &str) -> Version {
    let version_splitted: Vec<&str> = version.split(".").collect();
    (usize::from_str(version_splitted[0]).unwrap(), usize::from_str(version_splitted[1]).unwrap(), usize::from_str(version_splitted[2]).unwrap())
}

fn check_number_version(
    file_path: String,
    version_file: Option<Version>,
    version_compiler: Version
) -> Result<ReportCollection, Report> {
    use errors::{CompilerVersionError, NoCompilerVersionWarning};
    if let Some(required_version) = version_file {
        if required_version.0 == version_compiler.0 
        && required_version.1 == version_compiler.1 
        && required_version.2 <= version_compiler.2 {
            Ok(vec![])
        } else {
            let report = CompilerVersionError::produce_report(
                CompilerVersionError{
                    path: file_path,
                    required_version,
                    version: version_compiler
                }
            );
            Err(report)
        }
    } else {
        let report = NoCompilerVersionWarning::produce_report(
            NoCompilerVersionWarning{
                path: file_path,
                version: version_compiler
            }
        );
        Ok(vec![report])
    }
}
