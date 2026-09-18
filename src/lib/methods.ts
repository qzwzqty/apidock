/** HTTP 方法与配色（接口编辑、接口树、请求历史共用） */

export const METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

export const METHOD_COLORS: Record<string, string> = {
  GET: "bg-emerald-600",
  POST: "bg-orange-500",
  PUT: "bg-blue-500",
  PATCH: "bg-purple-500",
  DELETE: "bg-red-500",
  HEAD: "bg-slate-500",
  OPTIONS: "bg-slate-500",
};

export function methodColor(method: string): string {
  return METHOD_COLORS[method.toUpperCase()] ?? "bg-slate-500";
}

/** HTTP 状态码语义色：2xx 绿 / 3xx 黄 / 4xx+ 红 */
export function statusClass(status: number): string {
  if (status >= 200 && status < 300) return "text-green-500";
  if (status >= 400) return "text-red-400";
  return "text-yellow-500";
}