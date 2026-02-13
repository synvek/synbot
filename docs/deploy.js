import { execSync } from 'child_process';
import { readFileSync, existsSync } from 'fs';
import { join } from 'path';

console.log('========================================');
console.log('Synbot 文档部署脚本');
console.log('========================================\n');

// 检查 Node.js 版本
try {
  const nodeVersion = execSync('node --version', { encoding: 'utf8' }).trim();
  console.log(`1. Node.js 版本: ${nodeVersion}`);
} catch (error) {
  console.error('错误：Node.js 未安装或未在 PATH 中');
  process.exit(1);
}

// 检查 npm 包
console.log('\n2. 检查依赖项...');
try {
  execSync('npm list vitepress', { stdio: 'ignore' });
  console.log('✓ VitePress 已安装');
} catch (error) {
  console.log('正在安装依赖项...');
  execSync('npm install', { stdio: 'inherit' });
}

// 验证中文文档编码
console.log('\n3. 验证中文文档编码...');
const zhFiles = [
  'zh/index.md',
  'zh/getting-started/installation.md',
  'zh/getting-started/configuration.md',
  'zh/getting-started/running.md',
  'zh/getting-started/first-steps.md',
  'zh/user-guide/channels.md',
  'zh/user-guide/tools.md',
  'zh/user-guide/permissions.md',
  'zh/developer-guide/architecture.md',
  'zh/examples/basic-config.md'
];

let allValid = true;
for (const file of zhFiles) {
  const filePath = join(process.cwd(), file);
  if (existsSync(filePath)) {
    try {
      const content = readFileSync(filePath, 'utf8');
      // 检查是否包含中文字符
      const hasChinese = /[\u4e00-\u9fff]/.test(content);
      if (hasChinese) {
        console.log(`✓ ${file}: 包含中文字符，编码正确`);
      } else {
        console.log(`⚠ ${file}: 未检测到中文字符`);
      }
    } catch (error) {
      console.error(`✗ ${file}: 读取失败 - ${error.message}`);
      allValid = false;
    }
  } else {
    console.error(`✗ ${file}: 文件不存在`);
    allValid = false;
  }
}

if (!allValid) {
  console.error('\n错误：部分中文文档验证失败');
  process.exit(1);
}

// 构建文档
console.log('\n4. 构建文档...');
try {
  execSync('npm run build', { stdio: 'inherit' });
  console.log('✓ 构建成功！');
} catch (error) {
  console.error('✗ 构建失败');
  process.exit(1);
}

// 检查构建输出
console.log('\n5. 检查构建输出...');
const distPath = join(process.cwd(), '.vitepress', 'dist');
if (existsSync(distPath)) {
  console.log(`✓ 输出目录: ${distPath}`);
  
  // 检查构建的文件
  try {
    const files = execSync(`dir "${distPath}" /B`, { encoding: 'utf8' }).split('\n').filter(Boolean);
    console.log(`✓ 构建文件数量: ${files.length}`);
    
    // 检查 index.html
    const indexPath = join(distPath, 'index.html');
    if (existsSync(indexPath)) {
      const indexContent = readFileSync(indexPath, 'utf8');
      const hasTitle = indexContent.includes('Synbot Documentation');
      console.log(`✓ index.html: ${hasTitle ? '包含正确标题' : '标题可能不正确'}`);
    }
  } catch (error) {
    console.error(`✗ 检查构建输出失败: ${error.message}`);
  }
} else {
  console.error('✗ 构建输出目录不存在');
  process.exit(1);
}

console.log('\n========================================');
console.log('部署准备完成！');
console.log('========================================');
console.log('\n下一步：');
console.log('1. 启动预览服务器: npm run preview');
console.log('2. 部署到 GitHub Pages: npm run deploy');
console.log('3. 或手动上传到您的托管服务');
console.log('\n预览 URL: http://localhost:4173/docs/');
console.log('GitHub Pages URL: https://synbot.github.io/docs/');