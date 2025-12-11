// Snippets (快捷命令) 数据模型

use serde::{Deserialize, Serialize};

/// 单个快捷命令
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SnippetCommand {
    pub id: String,
    pub name: String,
    pub command: String,
    pub description: Option<String>,
    pub group_id: Option<String>, // 所属命令组 ID，None 表示未分组
    pub created_at: String,
}

impl Default for SnippetCommand {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            command: String::new(),
            description: None,
            group_id: None,
            created_at: String::new(),
        }
    }
}

/// 命令组 (支持嵌套)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SnippetGroup {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>, // 父级组 ID，None 表示顶级
    pub description: Option<String>,
    pub created_at: String,
}

impl Default for SnippetGroup {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            parent_id: None,
            description: None,
            created_at: String::new(),
        }
    }
}

/// Snippets 配置根结构
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct SnippetsConfig {
    pub groups: Vec<SnippetGroup>,
    pub commands: Vec<SnippetCommand>,
}

impl SnippetsConfig {
    /// 获取指定父级 ID 下的所有子组
    pub fn get_child_groups(&self, parent_id: Option<&str>) -> Vec<&SnippetGroup> {
        self.groups
            .iter()
            .filter(|g| g.parent_id.as_deref() == parent_id)
            .collect()
    }

    /// 获取指定组 ID 下的所有命令
    pub fn get_commands_in_group(&self, group_id: Option<&str>) -> Vec<&SnippetCommand> {
        self.commands
            .iter()
            .filter(|c| c.group_id.as_deref() == group_id)
            .collect()
    }

    /// 统计指定组下的子项数量（包含子组和命令）
    pub fn count_children(&self, group_id: &str) -> usize {
        let child_groups = self
            .groups
            .iter()
            .filter(|g| g.parent_id.as_deref() == Some(group_id))
            .count();
        let commands = self
            .commands
            .iter()
            .filter(|c| c.group_id.as_deref() == Some(group_id))
            .count();
        child_groups + commands
    }

    /// 获取组的名称，用于面包屑导航
    pub fn get_group_name(&self, group_id: &str) -> Option<&str> {
        self.groups
            .iter()
            .find(|g| g.id == group_id)
            .map(|g| g.name.as_str())
    }

    /// 构建面包屑路径（从根到当前组的名称列表）
    pub fn build_breadcrumb(&self, current_path: &[String]) -> Vec<(String, String)> {
        current_path
            .iter()
            .filter_map(|id| {
                self.get_group_name(id)
                    .map(|name| (id.clone(), name.to_string()))
            })
            .collect()
    }
}
