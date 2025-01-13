use include_dir::{include_dir, Dir};
use mcp_core::prompt::{Prompt, PromptArgument, PromptTemplate};

static PROMPTS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/src/developer/prompts");

pub fn create_unit_test_prompt() -> Prompt {
    let prompt_str = PROMPTS_DIR
        .get_file("unit_test.json")
        .map(|f| f.contents())
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .expect("Failed to read prompt template file");

    let template: PromptTemplate =
        serde_json::from_str(&prompt_str).expect("Failed to parse prompt template");

    let arguments = template
        .arguments
        .into_iter()
        .map(|arg| PromptArgument {
            name: arg.name.into(),
            description: arg.description.into(),
            required: arg.required,
        })
        .collect();

    Prompt::new(&template.id, &template.template, arguments)
}
