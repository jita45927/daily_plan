@echo off
chcp 65001 >nul
setlocal enabledelayedexpansion

echo ================================================
echo          每日计划 - 一键打包脚本
echo ================================================
echo.

set "PROJECT_DIR=%~dp0"
set "OUTPUT_DIR=%PROJECT_DIR%release"
set "TAURI_DIR=%PROJECT_DIR%src-tauri"

if not exist "%OUTPUT_DIR%" (
    mkdir "%OUTPUT_DIR%"
)

echo [1/3] 构建前端项目...
echo.
cd /d "%PROJECT_DIR%"
call npm run build
if %errorlevel% neq 0 (
    echo.
    echo 错误：前端构建失败！
    pause
    exit /b 1
)
echo 前端构建成功！
echo.

echo [2/3] 构建 Tauri 应用并生成 MSI 安装包...
echo.
cd /d "%PROJECT_DIR%"
npm run tauri build
if %errorlevel% neq 0 (
    echo.
    echo 错误：Tauri 构建失败！
    pause
    exit /b 1
)
echo Tauri 构建成功！
echo.

echo [3/3] 复制安装包到输出目录...
echo.
for /r "%TAURI_DIR%\target\release\bundle\msi" %%f in (*.msi) do (
    echo 复制: "%%f" -^> "%OUTPUT_DIR%"
    copy "%%f" "%OUTPUT_DIR%" >nul
)

if not exist "%OUTPUT_DIR%\*.msi" (
    echo.
    echo 警告：未找到 MSI 安装包！
) else (
    echo.
    echo ================================================
    echo          打包完成！
    echo ================================================
    echo 安装包位置: %OUTPUT_DIR%
    echo.
    dir "%OUTPUT_DIR%\*.msi"
    echo.
)

pause