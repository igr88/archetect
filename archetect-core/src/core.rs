use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use clap::crate_version;
use log::{debug, trace};
use semver::Version;

use crate::config::RuleAction;
use crate::rules::RulesContext;
use crate::system::{dot_home_layout, LayoutType, NativeSystemLayout, SystemLayout};
use crate::system::SystemError;
use crate::source::Source;
use crate::vendor::tera::{Context, Tera};
use crate::{ArchetectError, Archetype, ArchetypeError, RenderError};

pub struct Archetect {
    tera: Tera,
    paths: Rc<Box<dyn SystemLayout>>,
    offline: bool,
    headless: bool,
    switches: HashSet<String>,
}

impl Archetect {
    pub fn layout(&self) -> Rc<Box<dyn SystemLayout>> {
        self.paths.clone()
    }

    pub fn offline(&self) -> bool {
        self.offline
    }

    pub fn headless(&self) -> bool {
        self.headless
    }

    pub fn builder() -> ArchetectBuilder {
        ArchetectBuilder::new()
    }

    pub fn build() -> Result<Archetect, ArchetectError> {
        ArchetectBuilder::new().build()
    }

    pub fn template_engine(&self) -> &Tera {
        &self.tera
    }

    pub fn enable_switch<S: Into<String>>(&mut self, switch: S) {
        self.switches.insert(switch.into());
    }

    pub fn switches(&self) -> &HashSet<String> {
        &self.switches
    }

    pub fn load_archetype(&self, source: &str, relative_to: Option<Source>) -> Result<Archetype, ArchetypeError> {
        let source = Source::detect(self, source, relative_to)?;
        let archetype = Archetype::from_source(&source)?;
        Ok(archetype)
    }

    pub fn render_string(&mut self, template: &str, context: &Context) -> Result<String, RenderError> {
        match self.tera.render_str(template, &context.clone()) {
            Ok(result) => Ok(result),
            Err(err) => {
                Err(RenderError::StringRenderError {
                    string: template.to_owned(),
                    source: err,
                })
            }
        }
    }

    pub fn render_contents<P: AsRef<Path>>(&mut self, path: P, context: &Context) -> Result<String, RenderError> {
        let path = path.as_ref();
        let template = match fs::read_to_string(path) {
            Ok(template) => template,
            Err(error) => {
                return Err(RenderError::FileRenderIOError {
                    path: path.to_owned(),
                    source: error,
                });
            }
        };
        match self.tera.render_str(&template, &context.clone()) {
            Ok(result) => Ok(result),
            Err(error) => {
                Err(RenderError::FileRenderError {
                    path: path.into(),
                    source: error,
                })
            }
        }
    }

    pub fn render_directory<SRC: Into<PathBuf>, DEST: Into<PathBuf>>(
        &mut self,
        context: &Context,
        source: SRC,
        destination: DEST,
        rules_context: &mut RulesContext,
    ) -> Result<(), RenderError> {
        let source = source.into();
        let destination = destination.into();

        for entry in fs::read_dir(&source)? {
            let entry = entry?;
            let path = entry.path();

            let action = rules_context.get_source_action(path.as_path());

            if path.is_dir() {
                let destination = self.render_destination(&destination, &path, &context)?;
                debug!("Rendering   {:?}", &destination);
                fs::create_dir_all(destination.as_path())?;
                self.render_directory(context, path, destination, rules_context)?;
            } else if path.is_file() {
                let destination = self.render_destination(&destination, &path, &context)?;
                match action {
                    RuleAction::RENDER => {
                        if !destination.exists() {
                            debug!("Rendering   {:?}", destination);
                            let contents = self.render_contents(&path, &context)?;
                            self.write_contents(destination, &contents)?;
                        } else if rules_context.overwrite() {
                            debug!("Overwriting {:?}", destination);
                            let contents = self.render_contents(&path, &context)?;
                            self.write_contents(destination, &contents)?;
                        } else {
                            trace!("Preserving  {:?}", destination);
                        }
                    }
                    RuleAction::COPY => {
                        debug!("Copying     {:?}", destination);
                        self.copy_contents(&path, &destination)?;
                    }
                    RuleAction::SKIP => {
                        trace!("Skipping    {:?}", destination);
                    }
                }
            }
        }

        Ok(())
    }

    fn render_destination<P: AsRef<Path>, C: AsRef<Path>>(
        &mut self,
        parent: P,
        child: C,
        context: &Context,
    ) -> Result<PathBuf, RenderError> {
        let mut destination = parent.as_ref().to_owned();
        let child = child.as_ref();
        let name = self.render_path(&child, &context)?;
        destination.push(name);
        Ok(destination)
    }

    fn render_path<P: AsRef<Path>>(&mut self, path: P, context: &Context) -> Result<String, RenderError> {
        let path = path.as_ref();
        let filename = path.file_name().unwrap_or(path.as_os_str()).to_str().unwrap();
        match self.tera.render_str(filename, &context.clone()) {
            Ok(result) => Ok(result),
            Err(error) => {
                Err(RenderError::PathRenderError {
                    path: path.into(),
                    source: error,
                })
            }
        }
    }

    pub fn write_contents<P: AsRef<Path>>(&self, destination: P, contents: &str) -> Result<(), RenderError> {
        let destination = destination.as_ref();
        let mut output = File::create(&destination)?;
        output.write(contents.as_bytes())?;
        Ok(())
    }

    pub fn copy_contents<S: AsRef<Path>, D: AsRef<Path>>(&self, source: S, destination: D) -> Result<(), RenderError> {
        let source = source.as_ref();
        let destination = destination.as_ref();
        fs::copy(source, destination)?;
        Ok(())
    }

    pub fn version(&self) -> Version {
        Version::parse(crate_version!()).unwrap()
    }
}

pub struct ArchetectBuilder {
    layout: Option<Box<dyn SystemLayout>>,
    offline: bool,
    headless: bool,
    switches: HashSet<String>,
}

impl ArchetectBuilder {
    fn new() -> ArchetectBuilder {
        ArchetectBuilder {
            layout: None,
            offline: false,
            headless: false,
            switches: HashSet::new(),
        }
    }

    pub fn build(self) -> Result<Archetect, ArchetectError> {
        let layout = dot_home_layout()?;
        let paths = self.layout.unwrap_or_else(|| Box::new(layout));
        let paths = Rc::new(paths);

        Ok(Archetect {
            tera: crate::vendor::tera::extensions::create_tera(),
            paths,
            offline: self.offline,
            headless: self.headless,
            switches: self.switches,
        })
    }

    pub fn with_layout<P: SystemLayout + 'static>(mut self, layout: P) -> ArchetectBuilder {
        self.layout = Some(Box::new(layout));
        self
    }

    pub fn with_layout_type(self, layout: LayoutType) -> Result<ArchetectBuilder, SystemError> {
        let builder = match layout {
            LayoutType::Native => self.with_layout(NativeSystemLayout::new()?),
            LayoutType::DotHome => self.with_layout(dot_home_layout()?),
            LayoutType::Temp => self.with_layout(crate::system::temp_layout()?),
        };
        Ok(builder)
    }

    pub fn with_offline(mut self, offline: bool) -> ArchetectBuilder {
        self.offline = offline;
        self
    }

    pub fn with_headless(mut self, headless: bool) -> ArchetectBuilder {
        self.headless = headless;
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::system::{NativeSystemLayout, RootedSystemLayout};

    use super::*;

    #[test]
    fn test_explicit_native_paths() {
        let archetect = Archetect::builder()
            .with_layout(NativeSystemLayout::new().unwrap())
            .build()
            .unwrap();

        println!("{}", archetect.layout().catalog_cache_dir().display());
    }

    #[test]
    fn test_explicit_directory_paths() {
        let paths = RootedSystemLayout::new("~/.archetect/").unwrap();
        let archetect = Archetect::builder().with_layout(paths).build().unwrap();

        println!("{}", archetect.layout().catalog_cache_dir().display());
    }

    #[test]
    fn test_implicit() {
        let archetect = Archetect::build().unwrap();

        println!("{}", archetect.layout().catalog_cache_dir().display());

        std::fs::create_dir_all(archetect.layout().configs_dir()).expect("Error creating directory");
        std::fs::create_dir_all(archetect.layout().git_cache_dir()).expect("Error creating directory");
    }

    mod templating {
        use crate::Archetect;
        use crate::vendor::tera::Context;

        #[test]
        fn test_truncate_filter() {
            let mut archetect = Archetect::build().unwrap();
            let template = "{{ 'Jimmie' | truncate(length=1, end='') }}";
            let result = archetect.render_string(template, &Context::new()).unwrap();
            assert_eq!(&result, "J");
        }
    }
}
