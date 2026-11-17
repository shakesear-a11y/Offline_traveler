# Rust Offline Vector Map & Travel Planner

一个 **100% 纯 Rust 构建、零云端依赖、单文件 SQLite 持久化** 的离线矢量地图引擎、路径规划器与旅行手记桌面应用。

无需连接互联网或任何在线瓦片服务器，直接解析 OpenStreetMap 原始 `.osm.pbf` 数据，实现毫秒级空间渲染、A* 离线寻路、多日行程自动顺延排期、本地 EXIF 照片归档，以及可扩展的地理统计数据插件系统。

---

## 核心特性

### 1. 离线矢量地图与空间索引引擎 (`osm_index` & `map_renderer`)
* **原生 `.osm.pbf` 解析**：无需外部转换工具，直接在软件内导入任意省市或国家的 OSM PBF 数据包，自动提取水系、海岸线、绿地、分级路网、建筑轮廓与中英文地名标注。
* **SQLite R-Tree 空间索引**：采用本地 SQLite R-Tree 虚拟表与 WAL 模式，支持百万级地理要素的毫秒级视口（`MapBounds`）按需查询与分级抽稀渲染。
* **海陆分层底图**：支持加载全球/区域陆地轮廓 GeoJSON 作为永久底层，避免远景与沿海区域渲染穿帮。
* **自定义 POI 与 GPX 轨迹**：支持右键地图快速收藏自定义地点（持久化至本地数据库），以及一键导入并高亮渲染标准 `.gpx` 户外运动轨迹。

### 2. 纯 Rust 离线 A* 路径规划 (`pathfinder`)
* **本地路网拓扑图**：自动从已导入的 OSM 道路要素提取路网有向图，内置空间网格（Spatial Grid）实现点击坐标的毫秒级最近路口吸附。
* **多模式寻路**：支持 **驾车（Driving，按道路等级分配限速）** 与 **步行（Walking）** 两种模式，基于 Haversine 球面距离启发式 A* 算法实时计算最优路线、总里程与预计通行耗时。

### 3. 多日行程排期与离线游记媒体库 (`note_manager` & `photo_manager`)
* **多日时间轴自动顺延**：支持 D1–D5 多日行程切换，设定每日出发时间与各景点停留时长（Stay Minutes），系统自动结合路程耗时顺延推算全天每一站的精确到达与离开时刻。
* **本地照片归档与 EXIF 提取**：为每个行程节点撰写多行旅行手记并批量关联本地照片，自动读取照片 EXIF 原始拍摄时间（`DateTimeOriginal`），拷贝归档至本地 `trip_media/` 目录并生成 GPU 缩略图缓存。

### 4. 动态地图样式配置系统 (`style_config` & `style_panel`)
* **可视化实时调色**：内置 **经典浅色（Classic Light）**、**暗夜模式（Dark Night）** 与 **复古羊皮纸（Retro Paper）** 三套预设主题，支持通过界面拾色器与滑块实时调节各图层颜色及道路线宽。
* **外部 JSON 配置文件**：支持与根目录下的 `map_style.json` 双向同步，既可在界面调整后保存，也可直接编辑配置文件重载生效。

### 5. 可扩展地理统计数据插件系统 (`plugin`)
* **统一插件接口 (`MapDataPlugin`)**：支持在地图上叠加分级设色区域图（Choropleth）与比例气泡图（StatBubble），自动生成右下角动态图例卡片与鼠标悬停数据浮窗（Tooltip）。
* **内置与零代码数据接入**：
  * **人口出生率专题演示插件**：支持 2020–2026 年份滑块动态推演与气泡/网格模式切换。
  * **通用外部统计数据插件**：无需修改 Rust 代码，直接在界面导入任意带数值属性的 **`.geojson`** 行政区划文件或带经纬度列的 **`.csv`** 表格，自动计算四分位区间并映射色带渲染。

---

## 项目目录结构

```text
rust_map_routing/
├── Cargo.toml                 # 项目依赖配置
├── map_style.json             # 地图视觉样式配置文件 (自动生成)
├── ne_50m_land.geojson        # 全球/区域陆地轮廓底图 (需单独下载)
└── src/
    ├── main.rs                # 程序入口与数据库初始化
    ├── osm_index.rs           # 模块 1：.osm.pbf 解析与 SQLite R-Tree 空间建库
    ├── map_renderer.rs        # 模块 2：按范围渲染、动态符号、自定义 POI 与 GPX 轨迹
    ├── pathfinder.rs          # 模块 3：路网提取与 A* 最短时间路径规划
    ├── photo_manager.rs       # 模块 4：照片 EXIF 解析、媒体复制与缩略图纹理缓存
    ├── note_manager.rs        # 模块 5：多日行程管理、手记持久化与时间顺延算法
    ├── style_config.rs        # 样式配置序列化与预设主题定义
    ├── plugin/                # 专题数据插件体系
    │   ├── mod.rs             # 插件 Trait 协议与 PluginManager 注册中心
    │   ├── birth_rate.rs      # 内置人口出生率统计演示插件
    │   └── external_data.rs   # 通用 GeoJSON / CSV 外部统计数据加载插件
    └── ui/                    # egui 桌面图形界面层
        ├── mod.rs             # 主状态容器 OfflineMapApp 与后台异步协调
        ├── left_panel.rs      # 左侧控制栏 (PBF 索引、自定义 POI、GPX 轨迹)
        ├── right_panel.rs     # 右侧行程单、时间轴与照片手记编辑器
        ├── map_overlay.rs     # 地图叠加层 (地名标注、交互选点、统计气泡与图例)
        ├── style_panel.rs     # 地图样式拾色器与主题切换面板
        └── plugin_panel.rs    # 插件启用、参数滑块与外部统计文件导入面板
```

---

## 快速开始

### 1. 环境准备
* 安装 [Rust 工具链](https://rust-lang.org)（建议 Rust 1.75 及以上版本）。
* **Linux 系统依赖**（Ubuntu / Debian）：确保已安装图形窗口与字体开发库，以及中文字体以正常显示地图地名：
  ```bash
  sudo apt-get update
  sudo apt-get install -y libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libssl-dev libgtk-3-dev fonts-noto-cjk
  ```

### 2. 下载基础海陆轮廓底图
以下网址可以下载 Natural Earth 10m 精度陆地多边形 GeoJSON（约 1.8 MB）：
```URL
https://github.com/nvkelso/natural-earth-vector/blob/master/geojson/ne_10m_land.geojson
```

### 3. 准备离线地图数据包 (`.osm.pbf`)
前往 [Geofabrik 下载站](https://geofabrik.de) 下载任意地区或省份的 `.osm.pbf` 文件（例如 `guangdong-latest.osm.pbf` 或 `hunan-latest.osm.pbf`），将其放在项目根目录并重命名为 `default.osm.pbf`（或在启动后通过界面左上角按钮手动选择导入）。

### 4. 编译并启动
请务必使用 `--release` 模式运行，以获得最佳的 PBF 解析与 R-Tree 渲染性能：
```bash
cargo run --release
```

---

## 操作指南

* **地图浏览**：按住鼠标**左键拖动**平移地图；滚动**鼠标滚轮**缩放层级。
* **添加行程点**：在地图任意位置**右键点击**，或**左键点击**地图上的红色/橙色/蓝色城市与景点圆点，即可将该地点加入当前选中日期（D1–D5）的行程单并自动触发 A* 算路。
* **自定义 POI**：右键点击地图选中坐标后，在左侧栏 `2a. Custom POIs` 输入名称并点击 `Save POI` 即可永久标记在地图上。
* **导入外部统计 CSV 示例**：
  新建一个包含 `name,lon,lat` 及任意数值列的 `.csv` 文件：
  ```csv
  name,lon,lat,GDP_Billion,Birth_Rate
  长沙,112.9388,28.2282,1433.2,8.5
  广州,113.2644,23.1291,3035.5,11.2
  阳江,111.9822,21.8579,158.1,10.6
  ```
  在左侧栏展开 `4. Data Plugins (Statistics)`，点击 `Load .geojson / .csv File...` 导入即可自动生成分级气泡图与图例。

---

## 技术栈

* **GUI 框架**：[`eframe` / `egui`](https://github.com)
* **矢量地图引擎**：[`galileo` & `galileo-egui`](https://github.com)
* **空间数据库**：[`rusqlite`](https://github.com) (启用 SQLite R-Tree 扩展)
* **地理数据解析**：[`osmpbf`](https://github.com)、[`geojson`](https://github.com)、[`gpx`](https://github.com)
* **图像与元数据**：[`image`](https://github.com)、[`kamadak-exif`](https://github.com)

