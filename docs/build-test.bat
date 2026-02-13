@echo off
echo ========================================
echo Synbot 文档构建测试脚本
echo ========================================
echo.

echo 1. 检查 Node.js 版本
node --version
if %errorlevel% neq 0 (
    echo 错误：Node.js 未安装或未在 PATH 中
    exit /b 1
)

echo.
echo 2. 检查 npm 包
npm list vitepress
if %errorlevel% neq 0 (
    echo 警告：VitePress 可能未安装，正在安装...
    npm install
)

echo.
echo 3. 构建文档
echo 正在构建文档...
npm run build
if %errorlevel% neq 0 (
    echo 错误：构建失败
    exit /b 1
)

echo.
echo 4. 检查构建输出
if exist .vitepress\dist (
    echo 构建成功！输出目录：.vitepress\dist
    echo.
    echo 5. 检查文件编码
    echo 检查中文文档编码...
    powershell -Command "Get-Content docs/zh/index.md -Encoding UTF8 | Select-Object -First 5"
    echo.
    echo 6. 启动预览服务器
    echo 启动预览服务器...
    start "" "http://localhost:4173/docs/"
    npm run preview
) else (
    echo 错误：构建输出目录不存在
    exit /b 1
)