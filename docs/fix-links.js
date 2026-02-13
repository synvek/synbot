import { readFileSync, writeFileSync, readdirSync, statSync } from 'fs';
import { join } from 'path';

console.log('修复死链接脚本\n');

// 要修复的链接模式
const linkPatterns = [
  { pattern: /\/index\)/g, replacement: ')' }, // 移除 /index)
  { pattern: /\/index#/g, replacement: '#' }, // 移除 /index#
  { pattern: /\/index"/g, replacement: '"' }, // 移除 /index"
  { pattern: /\/index\s/g, replacement: ' ' }, // 移除 /index 后跟空格
];

// 递归处理所有 Markdown 文件
function processDirectory(dirPath) {
  const files = readdirSync(dirPath);
  
  for (const file of files) {
    const fullPath = join(dirPath, file);
    const stat = statSync(fullPath);
    
    if (stat.isDirectory()) {
      processDirectory(fullPath);
    } else if (file.endsWith('.md')) {
      processFile(fullPath);
    }
  }
}

function processFile(filePath) {
  try {
    let content = readFileSync(filePath, 'utf8');
    let originalContent = content;
    let changes = 0;
    
    // 应用所有修复模式
    for (const { pattern, replacement } of linkPatterns) {
      const matches = content.match(pattern);
      if (matches) {
        changes += matches.length;
        content = content.replace(pattern, replacement);
      }
    }
    
    // 修复特定格式的链接
    // 修复类似 [/zh/getting-started/configuration/index](/zh/getting-started/configuration/index)
    const linkPattern = /\[([^\]]+)\]\(([^)]+)\/index\)/g;
    const linkMatches = [...content.matchAll(linkPattern)];
    if (linkMatches.length > 0) {
      changes += linkMatches.length;
      content = content.replace(linkPattern, '[$1]($2)');
    }
    
    if (changes > 0) {
      writeFileSync(filePath, content, 'utf8');
      console.log(`✓ ${filePath}: 修复了 ${changes} 个链接`);
    }
  } catch (error) {
    console.error(`✗ ${filePath}: ${error.message}`);
  }
}

// 开始处理
console.log('正在扫描和修复链接...\n');
processDirectory(join(process.cwd(), 'en'));
processDirectory(join(process.cwd(), 'zh'));

console.log('\n完成！');