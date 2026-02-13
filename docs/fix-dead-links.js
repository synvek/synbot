import { readFileSync, writeFileSync, readdirSync, statSync } from 'fs';
import { join } from 'path';

console.log('修复死链接脚本 - 精确版本\n');

// 要修复的精确链接模式
const deadLinks = [
  // 英文文档链接
  '/docs/en/getting-started/configuration/index',
  '/docs/en/getting-started/running/index',
  '/docs/en/getting-started/installation/index',
  '/docs/en/user-guide/channels/index',
  '/docs/en/user-guide/tools/index',
  '/docs/en/user-guide/permissions/index',
  '/docs/en/user-guide/web-dashboard/index',
  '/docs/en/user-guide/troubleshooting/index',
  '/docs/en/user-guide/logging/index',
  '/docs/en/developer-guide/extending-tools/index',
  '/docs/en/developer-guide/adding-channels/index',
  '/docs/en/developer-guide/testing/index',
  '/docs/en/api-reference/index',
  '/docs/en/examples/multi-agent/index',
  '/docs/en/examples/permission-rules/index',
  '/docs/en/examples/custom-tools/index',
  
  // 中文文档链接
  '/docs/zh/getting-started/configuration/index',
  '/docs/zh/getting-started/running/index',
  '/docs/zh/getting-started/installation/index',
  '/zh/getting-started/configuration/index',
  '/zh/getting-started/running/index',
  '/zh/getting-started/installation/index',
  '/docs/zh/user-guide/channels/index',
  '/docs/zh/user-guide/tools/index',
  '/docs/zh/user-guide/permissions/index',
  '/docs/zh/user-guide/web-dashboard/index',
  '/zh/user-guide/channels/index',
  '/zh/user-guide/tools/index',
  '/zh/user-guide/permissions/index',
  '/zh/user-guide/web-dashboard/index',
  '/zh/user-guide/troubleshooting/index',
  '/zh/user-guide/logging/index',
  '/docs/zh/developer-guide/extending-tools/index',
  '/docs/zh/developer-guide/adding-channels/index',
  '/docs/zh/developer-guide/testing/index',
  '/docs/zh/api-reference/index',
  '/docs/zh/examples/multi-agent/index',
  '/docs/zh/examples/permission-rules/index',
  '/docs/zh/examples/custom-tools/index',
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
    
    // 修复每个死链接
    for (const deadLink of deadLinks) {
      const fixedLink = deadLink.replace(/\/index$/, '');
      
      // 修复 Markdown 链接格式 [text](link)
      const linkPattern = new RegExp(`\\[([^\\]]+)\\]\\(${deadLink.replace(/\//g, '\\/')}\\)`, 'g');
      const matches = content.match(linkPattern);
      if (matches) {
        changes += matches.length;
        content = content.replace(linkPattern, `[$1](${fixedLink})`);
      }
      
      // 修复纯链接格式 (link)
      const pureLinkPattern = new RegExp(`\\(${deadLink.replace(/\//g, '\\/')}\\)`, 'g');
      const pureMatches = content.match(pureLinkPattern);
      if (pureMatches) {
        changes += pureMatches.length;
        content = content.replace(pureLinkPattern, `(${fixedLink})`);
      }
    }
    
    if (changes > 0) {
      writeFileSync(filePath, content, 'utf8');
      console.log(`✓ ${filePath}: 修复了 ${changes} 个链接`);
      
      // 显示修复的示例
      const lines = content.split('\n');
      const fixedLines = lines.filter(line => line.includes('](') && !line.includes('/index)'));
      if (fixedLines.length > 0) {
        console.log(`  示例: ${fixedLines[0].substring(0, 80)}...`);
      }
    }
  } catch (error) {
    console.error(`✗ ${filePath}: ${error.message}`);
  }
}

// 开始处理
console.log('正在扫描和修复死链接...\n');
processDirectory(join(process.cwd(), 'en'));
processDirectory(join(process.cwd(), 'zh'));

console.log('\n完成！');