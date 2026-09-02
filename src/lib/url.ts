/** URL 的 host 模板：占位符由环境注入 */
export const HOST_TEMPLATE = "{{host}}";

/**
 * 从接口文档 URL 中拆出「可编辑的路径部分」。
 * - URL 以 {{host}} 开头 → 去掉模板
 * - URL 以当前环境 host 开头（旧数据完整 URL）→ 去掉 host
 * - 其他（相对路径或未知服务地址）→ 原样返回，由用户改为路径
 */
export function splitUrlPath(url: string, host: string): string {
  if (url.startsWith(HOST_TEMPLATE)) return url.slice(HOST_TEMPLATE.length);
  if (host && url.startsWith(host)) return url.slice(host.length);
  return url;
}

/**
 * 组装为模板 URL：host 一律来自环境（{{host}}），用户只提供路径。
 * 粘贴完整 URL 时自动剥离协议与 host，仅保留路径+查询。
 */
export function buildTemplateUrl(path: string): string {
  let p = path.trim();
  if (/^https?:\/\//i.test(p)) {
    try {
      const u = new URL(p);
      p = u.pathname + u.search;
    } catch {
      // 解析失败则原样保留
    }
  }
  if (!p.startsWith("/")) p = "/" + p;
  return HOST_TEMPLATE + p;
}

/**
 * 发送前规范化：任何形态的接口 URL 都统一为 {{host}}/路径。
 * - 裸路径（旧数据，如 /api/login）→ 自动补 {{host}} 前缀
 * - {{host}}/路径 → 幂等不变
 * - 完整地址（粘贴或旧数据）→ 剥离协议与 host，仅保留路径
 * - 脏数据单花括号 {host}/路径 → 剥离前缀
 */
export function normalizeUrlForSend(url: string, host: string): string {
  let p = splitUrlPath(url, host);
  if (p.startsWith("{host}")) p = p.slice("{host}".length);
  return buildTemplateUrl(p);
}