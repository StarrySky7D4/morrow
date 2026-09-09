Morrow — Web 预构建包

解压后，将 morrow-web 文件夹中的全部内容部署到静态网站根目录。
保留 assets、canvaskit、图标以及 LICENSE / NOTICE 等文件。
请通过 HTTP / HTTPS 访问，不要直接双击 index.html 使用 file:// 打开。
本地预览可在此目录运行 Python 3：python -m http.server 8765
然后访问 http://localhost:8765。

内容保存在当前浏览器、当前站点来源的 localStorage / IndexedDB 中。
没有账户或云同步；清除站点数据会移除该浏览器保存的内容。
剪贴板取决于浏览器支持与权限，线上部署请使用 HTTPS。
新建灵感支持 Markdown 编辑与实时预览，以及浏览器可读的富文本、表格、图片和文件。
Windows 原生 Office OLE／矢量对象提取仅桌面版提供；Web 可使用「导入文件」。
磨砂、超透与液体玻璃之间支持连续过渡；侧边栏与设置栏可带动画收起／展开。
默认、纯色、纹理、透明四种画布均可独立开启液体玻璃效果。
透明画布保持中央透明，仅添加边缘高光，透出网页背景，不会透出操作系统桌面。
浏览器无法自动寻找音频旁的歌词文件，请同时选择音频和同名 LRC，
或在播放列表手动导入。音视频格式支持取决于浏览器。

本项目原创内容采用 Apache License 2.0，见 LICENSE 和 NOTICE。
第三方依赖保留各自许可证，见 THIRD_PARTY_NOTICES.txt 和生成的 NOTICES。
