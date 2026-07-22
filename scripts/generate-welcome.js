import fs from 'fs';
import path from 'path';

// 读取图片
const imagePath = path.join(path.dirname(import.meta.url).replace('file:///', ''), '../public/welcome.jpg');
const imageBuffer = fs.readFileSync(imagePath);
const base64Image = imageBuffer.toString('base64');

// 生成 HTML 内容
const htmlContent = `<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8" />
<meta name="viewport" content="width=device-width, initial-scale=1.0" />
<title>欢迎</title>
<style>
* { margin: 0; padding: 0; box-sizing: border-box; }
html, body { width: 100%; height: 100%; overflow: hidden; }
.welcome-container {
  width: 100%;
  height: 100%;
  background-image: url(data:image/jpeg;base64,${base64Image});
  background-size: cover;
  background-position: center;
  position: relative;
}
.progress-section {
  position: absolute;
  bottom: 40px;
  left: 50%;
  transform: translateX(-50%);
  width: 300px;
}
.progress-text {
  text-align: center;
  color: rgba(255, 255, 255, 0.8);
  font-size: 14px;
  margin-bottom: 8px;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
}
.progress-bar-container {
  width: 100%;
  height: 4px;
  background: rgba(0, 0, 0, 0.3);
  border-radius: 2px;
  overflow: hidden;
}
.progress-bar {
  height: 100%;
  background: rgba(255, 255, 255, 0.9);
  border-radius: 2px;
  width: 0%;
  transition: width 0.3s ease;
}
</style>
</head>
<body>
<div class="welcome-container">
  <div class="progress-section">
    <div class="progress-text">程序加载中......</div>
    <div class="progress-bar-container">
      <div class="progress-bar" id="progressBar"></div>
    </div>
  </div>
</div>
<script>
const progressBar = document.getElementById('progressBar');
progressBar.style.width = '10%';
</script>
</body>
</html>`;

// 写入 HTML 文件
const outputPath = path.join(path.dirname(import.meta.url).replace('file:///', ''), '../welcome.html');
fs.writeFileSync(outputPath, htmlContent, 'utf8');

console.log('welcome.html 生成完成，图片已嵌入');
