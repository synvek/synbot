# 办公交付物预览

agent 产出的文档都以文件形式落下——`report.docx`、`budget.xlsx`、`deck.pptx`、`chart.pdf`。工作区的 **交付物** tab 把这些文件变成可以在面板里直接读的内容，而不必先下载一个二进制文件再猜里面是什么。

所有处理都在本机完成：synbot 自己读 OOXML 包（`.docx` 本质是若干 XML 部件打成的 ZIP），抽取文字，其余交给浏览器渲染。不需要 LibreOffice，没有转换服务，也没有新增外部依赖。

## 会列出哪些文件

tab 会列出项目中所有扩展名能归类的文件：

| 类型 | 扩展名 |
|------|--------|
| `docx` | `docx` |
| `xlsx` | `xlsx`、`xlsm` |
| `pptx` | `pptx` |
| `pdf` | `pdf` |
| `csv` | `csv`、`tsv` |
| `markdown` | `md`、`markdown` |
| `html` | `html`、`htm` |
| `image` | `png`、`jpg`、`jpeg`、`gif`、`webp`、`svg`、`bmp` |
| `text` | `txt`、`log`、`json`、`yml`、`yaml`、`toml` |

其余文件（源码、压缩包、旧版 `.doc`/`.xls`/`.ppt`）不算交付物，仍留在「文件」tab 里。

列表会递归遍历项目，并且：

- 跳过隐藏目录与构建产物（`.git`、`node_modules`、`target`、`dist`、`build` 等）；
- 跳过符号链接——指向项目外的链接不能作为交付物出现；
- 最多收集 500 份文档、检查 20 000 个条目、下钻 12 层目录，超出时标记 `truncated`；
- 按修改时间从新到旧排序，agent 刚写完的文件排在最上面。

## 预览

点击一行即可预览；每行（以及预览标题栏）都有 **导出** 按钮，下载原始字节。

| 类型 | 预览内容 |
|------|----------|
| `docx` | 文档属性中的标题/作者、字数与段数，然后按文档顺序显示段落。表格按「每个单元格一段」读取。 |
| `xlsx` | 每个 sheet 一张表，带 sheet 切换、行号，以及已使用单元格的文本。 |
| `pptx` | 每页幻灯片一张卡片：首行作为标题，其余行作为正文条目。 |
| `pdf`、`image` | 交给浏览器渲染（服务端只负责传输字节）。 |
| `csv`、`markdown`、`html`、`text` | 直接显示文本，最多 40 000 字符。 |

值按存储形态显示：日期单元格显示的是序列号，公式显示 Excel 缓存的值，合并单元格不会重复内容。格式、字体、图片、图表与版式不在范围内，详见下文「边界」。

## 限额

读取过程中有硬性上限，避免单个异常或超大文件拖垮 daemon：

| 上限 | 值 |
|------|-----|
| 可打开的文件大小 | 64 MB |
| 单个解压后的 ZIP 条目 | 32 MB |
| 纯文本字符数 | 40 000 |
| 段落数（`docx`） | 300 |
| sheet 数 / 每个 sheet 行数 / 每行列数（`xlsx`） | 20 / 200 / 40 |
| 共享字符串数（`xlsx`） | 5 000 |
| 幻灯片数 / 每页行数（`pptx`） | 60 / 40 |
| 通过面板下载的大小 | 64 MB |

触顶时预览会明确提示，而导出得到的始终是完整文件。

## HTTP API

| 端点 | 结果 |
|------|------|
| `GET /api/projects/{id}/deliverables?path=output` | `{ path, entries: [{ name, path, kind, size, modified_ms }], truncated, scanned }`。`path` 把遍历限定在子目录。 |
| `GET /api/projects/{id}/preview?path=output/report.docx` | `{ path, name, preview }`，结构见下。无法解析的文件返回 `400` 并附原因，文件不存在返回 `404`。 |
| `GET /api/projects/{id}/download?path=output/report.docx` | 原始字节，带正确 MIME 类型、`Content-Disposition: attachment`、`Cache-Control: no-store`。 |

`preview` 按类型只携带一种载荷，其余字段省略：

```json
{
  "kind": "docx",
  "size": 18234,
  "passthrough": false,
  "truncated": false,
  "docx": {
    "title": "Q3 report",
    "author": "Ada",
    "paragraphs_total": 12,
    "words": 340,
    "paragraphs": ["Revenue grew 12%", "Costs fell 4%"],
    "truncated": false
  }
}
```

`passthrough: true` 表示前端应自行取 `/download` 并渲染（PDF、图片）；`text` 承载纯文本类型；`xlsx` 与 `pptx` 分别携带 `sheets` 与 `slides`。路径与「文件」tab 共用同一套校验：拒绝 `..` 与绝对路径，符号链接解析到项目外也会被拒绝。

## 边界

- **没有视觉保真。** 预览回答的是「这份文档写了什么」。字体、样式、颜色、图片、图表、合并单元格、分页与数字格式都不会还原。
- **没有渲染服务。** 不做任何转 PDF / 转图片，也不启动无头办公套件。PDF 与图片能用，只是因为浏览器本身能渲染。
- **仅支持 OOXML。** 旧版二进制 `.doc`/`.xls`/`.ppt` 以及带宏的 `.docm`/`.pptm` 不做预览；请另存为 `docx`/`xlsx`/`pptx`。
- **截断只发生在内存里。** 限额不会改动磁盘上的文件——导出的字节与 agent 写入的完全一致。

## 实现位置

- `src/office/mod.rs` — 类型识别、限额、预览分发、纯文本读取。
- `src/office/zip.rs` — 最小 ZIP 读取器（中央目录、stored + deflate、不支持 ZIP64、条目上限）。
- `src/office/xml.rs` — 最小 XML tokenizer（剥离命名空间前缀、解码实体、遇到畸形输入停止遍历而不是 panic）。
- `src/office/docx.rs`、`src/office/xlsx.rs`、`src/office/pptx.rs` — 每种格式一个抽取器。
- `src/office/testing.rs` — 造包工具（`docx`、`xlsx`、`pptx`），测试用真实 ZIP 字节。
- `src/web/handlers/delivery.rs` — 三个 REST handler；阻塞读取放在 `web::block` 上。
- `web/src/components/DeliverablePanel.tsx` — 交付物 tab、预览与导出。
