项目名称:Fluen
版本:0.0.1-beta
时间:2026年7月
运行在Windows和MacOS、Linux
本项目是使用LLM、OCR等技术辅助学术创作的平台，基于tauri开发，使用Vue3和Rust开发
pnpm管理前端依赖和运行项目
项目部分场景需要同时维护两套API（本地模式（使用本地的函数以及API服务）和云端协同模式（使用云端服务，需要注册账号））
设计风格参照DESIGN.md的描述
代码必须严格按照模块化设计，拆分合理、清晰，代码清晰、简洁、规范，整体必须易于维护、拓展和使用，易于复用，设计无冗余，单文件不超过800行。
\crates是子仓库的内容，如果需要更改必须告知我，并获得授权
用户数据存储路径（遵循各平台官方规范）
Linux (XDG规范):
  配置: $XDG_CONFIG_HOME/Fluen/ (回退 ~/.config/Fluen/)
  数据: $XDG_DATA_HOME/Fluen/ (回退 ~/.local/share/Fluen/)
  缓存: $XDG_CACHE_HOME/Fluen/ (回退 ~/.cache/Fluen/)
Windows:
  配置: %APPDATA%\Fluen\
  数据: %LOCALAPPDATA%\Fluen\
  缓存: %LOCALAPPDATA%\Fluen\Cache\
macOS:
  配置/数据: ~/Library/Application Support/com.wppcp.fluen/
  缓存: ~/Library/Caches/com.wppcp.fluen/
路径解析统一由 src-tauri/src/platform.rs 提供（fluen_config_dir / fluen_data_dir / fluen_cache_dir / fluen_documents_dir）


设计逻辑
每个论文都是一个完整的Git项目，其固定的文件结构为
project_name/
-config.yaml            #记录文章标题、作者等信息（同时标注Fluen文件版本）
-references/            #参考文献
--references-index.json       #用于记录参考文献id与title的映射，以及各文献的添加时间、来源网站、AI摘要
--md/
---ref-16位UUID4.md     #存储由PDF格式转化为的MD文献
--translation-cache/   # 翻译模块存储的数据
--raw/               #参考文献源文件（PDF、EPUB等等格式）
--wiki/              #知识库（wiki.md 规范）
---index.md          #全局索引（始终维护）
---index.db          #SQLite 索引（FTS5 + 可选向量）
---concepts/         #概念页 wiki-ID-title.md
---entities/         #实体页 wiki-ID-title.md
---summaries/        #综述页 wiki-ID-title.md（对应文献）
-data/                  #数据源
--experiments/          #实验类数据
--questionnaires/       #问卷类数据
-manuscript/            #需要编写的论文
--sections/             #章节主体(将论文拆分为摘要、引言等大段拆分为单独的MD文件)
---sections.json        #用于记录章节的id与章节顺序信息
---sec-16位UUID4.md     #论文文本数据，YAML front matter 直接内嵌标题、创建时间、更新时间
--assets/               #章节资源（图标、CSV等等额外数据）
完成后的结尾必须加上：主人！我完成了，请检查