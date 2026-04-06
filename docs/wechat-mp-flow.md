# 微信公众号后台（mp.weixin.qq.com）抓包流程参考

本文档基于 chrome-devtools MCP 实际走通一次完整的"扫码登录 → 进编辑器 → 超链接查找公众号 → 列出文章 → 翻页"流程，记录每个步骤对应的 HTTP 请求、参数、响应结构和关键鉴权载荷。Rust CLI 应按此文档高保真复刻浏览器行为。

## 0. 全局约定

- **Base URL**: `https://mp.weixin.qq.com`
- **User-Agent**（建议按真实 Chrome 桌面端固定）:
  `Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36`
- **通用 Headers**（除登录前的请求外，所有业务请求都要带上）:
  - `Referer: https://mp.weixin.qq.com/`（编辑器内请求 Referer 为编辑器 URL，见 §5）
  - `X-Requested-With: XMLHttpRequest`
  - `Accept: */*`
  - `Accept-Language: zh,zh-CN;q=0.9`
  - `Origin: https://mp.weixin.qq.com`（仅 POST 时）
  - `Content-Type: application/x-www-form-urlencoded; charset=UTF-8`（仅 POST 时）
- **通用 Query 参数**（登录后所有 cgi-bin 接口都会带）:
  - `token`：登录后从重定向 URL 提取（见 §3）
  - `lang=zh_CN`
  - `f=json`
  - `ajax=1`
  - `fingerprint`：32 位 hex，由前端基于浏览器指纹生成，**同一 session 内保持不变即可**（本次实测值：`ca2817976fe6fa8a3dfc7a5c14813c38`）。可直接生成一次随机 hex 固定复用。
- **Cookie jar**：必须全程复用同一个 cookie jar。登录成功后 Tencent 会写入 `slave_sid / slave_user / data_ticket / bizuin / data_bizuin / slave_bizuin / xid / rand_info` 等，后续所有请求都依赖这些 cookie。

---

## 1. 第一步：获取登录页 + 开启扫码会话

### 1.1 GET `/`
打开首页，取到初始 cookie（`uuid`、`_qimei_*`、`mm_lang` 等）。

### 1.2 POST `/cgi-bin/bizlogin?action=startlogin`
**请求体**（`application/x-www-form-urlencoded`）：
```
userlang=zh_CN
redirect_url=
login_type=3
sessionid=177539514991040           # 毫秒时间戳 + 随机尾数，本次为 String("177"+ts)
fingerprint=ca2817976fe6fa8a3dfc7a5c14813c38
token=
lang=zh_CN
f=json
ajax=1
```
**响应**：
```json
{"base_resp":{"err_msg":"ok","ret":0}}
```
副作用：Set-Cookie `uuid=...`，并清空若干旧会话 cookie。

### 1.3 获取二维码图片
二维码图片由前端通过下列 URL 拉取（GET）：
```
/cgi-bin/scanloginqrcode?action=getqrcode&random={rand}
```
（本次请求未单独抓到，因为图片通过 `<img>` 触发；Rust 端可直接 GET 该 URL 保存为 PNG/JPEG 供用户扫描。）

---

## 2. 第二步：轮询扫码状态

### 2.1 GET `/cgi-bin/scanloginqrcode?action=ask`
**Query**:
```
action=ask
fingerprint={fp}
token=
lang=zh_CN
f=json
ajax=1
```
前端每 ~1s 调一次，直到 `status` 发生变化。

**响应示例**（未扫码）：
```json
{"acct_size":0,"base_resp":{"err_msg":"ok","ret":0},"binduin":0,"status":0,"user_category":0}
```
`status` 取值经验：
- `0`：等待扫码
- `4`：已扫码，待手机端点确认
- `1`：手机端已确认 → 应立刻停止轮询并发起 §3 登录完成请求
- `5` / `6`：二维码过期/失效 → 需重新走 §1.2

---

## 3. 第三步：完成登录并提取 token

### 3.1 POST `/cgi-bin/bizlogin?action=login`
**请求体**:
```
userlang=zh_CN
redirect_url=
cookie_forbidden=0
cookie_cleaned=0
plugin_used=0
login_type=3
fingerprint={fp}
token=
lang=zh_CN
f=json
ajax=1
```
**响应**：`Set-Cookie` 里返回整套鉴权 cookie：
```
rand_info=CAESIP...
slave_bizuin=3685040628
data_bizuin=3685040628
bizuin=3685040628
data_ticket=aw8XZzPHyfW8bqzgtfXbe...
slave_sid=ZU80VDBNWVVlM2...
slave_user=gh_4bc79e2009ba
xid=d33001fb8cb99bfe3c63a00786a12526
```
同时响应体中会包含 `redirect_url` 字段，形如：
```
/cgi-bin/home?t=home/index&lang=zh_CN&token=362631694
```
**从该 URL 的 `token` query 参数中提取 token**，这是后续所有接口的通行证。

### 3.2 GET `/cgi-bin/home?t=home/index&lang=zh_CN&token={token}`
用 HTML 方式请求一次首页作为"正常用户"验证。Rust 端只需跟随这次跳转即可（不强制解析 HTML）。

---

## 4. 第四步：进入文章编辑器（取得 Referer 资格）

### 4.1 GET `/cgi-bin/appmsg?t=media/appmsg_edit_v2&action=edit&isNew=1&type=77&createType=0&token={token}&lang=zh_CN&timestamp={ms}`

- `type=77`：普通图文文章
- `isNew=1`、`createType=0`：新建空白文章
- `timestamp`：当前毫秒时间戳

**作用**：该 URL 是后续超链接 / 搜索 / 列表接口的 `Referer` 值。**Rust CLI 不必真正渲染页面，但必须把这个 URL 当作后续 xhr 的 Referer 发送**，否则 CGI 会以权限不足拒绝。

---

## 5. 第五步：搜索目标公众号

### 5.1 GET `/cgi-bin/searchbiz?action=search_biz`
**Query**:
```
action=search_biz
begin=0
count=5
query={URL-encoded 公众号名称 / 微信号}
fingerprint={fp}
token={token}
lang=zh_CN
f=json
ajax=1
```
**Headers**:
- `Referer: https://mp.weixin.qq.com/cgi-bin/appmsg?t=media/appmsg_edit_v2&...`
- `X-Requested-With: XMLHttpRequest`

**响应结构**（实测 `query=人民日报`）：
```json
{
  "base_resp": {"ret": 0, "err_msg": "ok"},
  "list": [
    {
      "fakeid": "MjM5MjAxNDM4MA==",
      "nickname": "人民日报",
      "alias": "rmrbwx",
      "round_head_img": "http://mmbiz.qpic.cn/...",
      "service_type": 0,
      "signature": "参与、沟通、记录时代。",
      "username": "",
      "verify_status": 3
    },
    ...
  ],
  "total": 5
}
```
**关键字段**：`fakeid` 是下一步列表接口必需的公众号定位符。

---

## 6. 第六步：拉取目标公众号的文章列表（含翻页）

### 6.1 GET `/cgi-bin/appmsgpublish?sub=list&sub_action=list_ex`
**Query**:
```
sub=list
search_field=null
begin=0            # 翻页起始 offset，翻页步长 = count
count=5            # 每页数量，浏览器默认 5
query=             # 可选：在该号内按标题过滤
fakeid={URL-encoded fakeid}   # 注意 == 要编码成 %3D%3D
type=101_1
free_publish_type=1
sub_action=list_ex
fingerprint={fp}
token={token}
lang=zh_CN
f=json
ajax=1
```
**Headers** 同 §5.1。

**翻页**：第二页 `begin=5&count=5`，第三页 `begin=10&count=5`，以此类推。

### 6.2 响应结构（**双层 JSON，要二次 parse**）

外层：
```json
{
  "base_resp": {"err_msg": "ok", "ret": 0},
  "is_admin": true,
  "publish_page": "<stringified JSON>"
}
```

`publish_page` 是字符串，JSON.parse 后得到：
```json
{
  "total_count": 40111,           // 该号全部文章数（用于计算翻页上限）
  "publish_count": 21,
  "masssend_count": 40090,
  "publish_list": [
    {
      "publish_type": 101,
      "publish_info": "<stringified JSON>"   // 再次需要二次 parse
    },
    ...
  ]
}
```

`publish_info` 再次 parse 后含有 `appmsgex` 数组，每个元素才是单篇文章：
```json
{
  "appmsgex": [
    {
      "aid": "2667006182_1",
      "appmsgid": 2667006182,
      "itemidx": 1,
      "title": "祝贺孙颖莎！",
      "link": "https://mp.weixin.qq.com/s/lathoSifoD-d-6Z3m45kUw",
      "cover": "https://mmbiz.qpic.cn/.../0?wx_fmt=jpeg",
      "digest": "",
      "update_time": 1775394817,
      "create_time": 1775394817,
      "author_name": "",
      "is_deleted": false,
      ...
    }
  ]
}
```

**Rust CLI 抓取单元**: `{title, link, cover, digest, update_time, author_name, appmsgid, aid}`。`link` 就是公众号文章的最终可爬取 URL，可直接喂给 `spider-rs` 的 Scrape。

---

## 7. 错误处理与反爬注意点

1. **频率限制**：`searchbiz` 和 `appmsgpublish` 都有频控。经验值：同一 token 下每分钟 ≤ ~20 次搜索、每分钟 ≤ ~60 次列表拉取。建议请求间插入 1~3s 抖动。触发时 `base_resp.ret` 会返回 `200013`（freq control）或 `200003`（session expired）。
2. **Session 失效**：`ret=200003` → 重新走 §1~§3。
3. **cookie 完整性**：切勿丢失 `slave_sid / data_ticket / slave_user`，任何一项缺失都会被判定未登录。
4. **Referer 必备**：编辑器内的 xhr 接口（searchbiz / appmsgpublish）如果 Referer 不是 `appmsg?t=media/appmsg_edit_v2...` 会被直接拒。
5. **token 可复用**：token 生命周期约等于登录 session，通常 2 小时内有效，过期后 cgi 返回 `ret=200003`，需重新扫码。
6. **响应的 `logicret` / `retkey` header**：业务成功 `logicret=0`，可用于快速判错。
7. **badjs / webreport / mplog** 等上报接口 Rust 端可以完全忽略。

---

## 8. 流程时序一览（Rust CLI 规划）

```
login
├─ GET  /                                               # 取初始 cookie
├─ POST /cgi-bin/bizlogin?action=startlogin             # 启动扫码会话
├─ GET  /cgi-bin/scanloginqrcode?action=getqrcode       # 下载二维码图片并展示给用户
├─ loop GET /cgi-bin/scanloginqrcode?action=ask         # 1s/次 轮询 status
└─ POST /cgi-bin/bizlogin?action=login                  # 拿 redirect_url → 提取 token

crawl <biz_name>
├─ GET  /cgi-bin/appmsg?t=media/appmsg_edit_v2&...      # 建立 Referer 资格
├─ GET  /cgi-bin/searchbiz?query=<biz_name>             # 拿到 fakeid
└─ loop GET /cgi-bin/appmsgpublish?fakeid=&begin=N      # 翻页直到 begin>=total_count

scrape <url>
└─ spider-rs Scrape → markdown
```

---

## 9. 本次实测抓包原始样本（脱敏）

- token：`362631694`
- bizuin（登录账号 uin）：`3685040628`
- fingerprint：`ca2817976fe6fa8a3dfc7a5c14813c38`
- 目标号：人民日报
  - fakeid：`MjM5MjAxNDM4MA==`
  - alias：`rmrbwx`
  - total_count：`40111`
  - 第一页样本文章：`祝贺孙颖莎！` → `https://mp.weixin.qq.com/s/lathoSifoD-d-6Z3m45kUw`
- 翻页验证：`begin=0&count=5` → `begin=5&count=5` 正常返回下一页。
