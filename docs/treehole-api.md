# PKU Treehole API Documentation

## Overview

北大树洞 (PKU Treehole) is an anonymous forum for Peking University students.

- **Base URL**: `https://treehole.pku.edu.cn`
- **API Base**: `/chapi/api/v3/`
- **Authentication**: JWT Bearer Token + XSRF Token + Session Cookie

---

## Authentication Flow

### Step 1: IAAA Login (PKU Unified Authentication)

The treehole uses PKU's IAAA system for authentication. Two methods are supported:

#### Method A: Username/Password Login

1. **Get RSA Public Key**
   ```
   GET https://iaaa.pku.edu.cn/iaaa/getPublicKey.do
   Response: { "success": true, "key": "-----BEGIN PUBLIC KEY-----\n...\n-----END PUBLIC KEY-----" }
   ```

2. **Encrypt Password**
   - Use RSA public key to encrypt the password with JSEncrypt (PKCS#1 v1.5)

3. **Submit Login**
   ```
   POST https://iaaa.pku.edu.cn/iaaa/oauthlogin.do
   Content-Type: application/x-www-form-urlencoded

   appid=PKU+Helper
   userName=<student_id>
   password=<rsa_encrypted_password>
   randCode=
   smsCode=
   otpCode=
   redirUrl=https://treehole.pku.edu.cn/chapi/cas_iaaa_login?version=3&uuid=<random_hex>&plat=web

   Response (success): { "success": true, "token": "<iaaa_token>" }
   Response (failure): { "success": false, "errors": { "code": "E01", "msg": "..." } }
   ```

4. **Redirect to Treehole**
   ```
   GET https://treehole.pku.edu.cn/chapi/cas_iaaa_login?version=3&uuid=<uuid>&plat=web&_rand=<random>&token=<iaaa_token>
   
   Response: 302 redirect to /ch/web/iaaa_success?is_mobile=0&token=<jwt_token>&expires_in=<timestamp>&uid=<student_id>
   Sets cookies: pku_token, pku_expires_in, pku_uid, XSRF-TOKEN, _session
   ```

#### Method B: QR Code Login

1. **Generate QR Code**
   ```
   GET https://iaaa.pku.edu.cn/iaaa/genQRCode.do?userName=&_rand=<random>&appId=PKU+Helper
   Response: QR code image (PNG)
   ```
   - User scans QR code with "北京大学" App

2. **Poll for Login Result** (every 3 seconds, max 60 attempts)
   ```
   POST https://iaaa.pku.edu.cn/iaaa/oauthlogin4QRCode.do
   Content-Type: application/x-www-form-urlencoded

   appId=PKUApp
   issuerAppId=iaaa
   targetAppId=PKU+Helper
   redirectUrl=<redirect_url>

   Response (waiting): { "success": false, "errors": { "code": "E10", "msg": "无有效绑定" } }
   Response (success): { "success": true, "token": "<iaaa_token>" }
   ```

3. Same redirect as Method A Step 4.

### Step 2: SMS Verification (First Login / Periodic)

After IAAA login, the treehole may require SMS verification:

1. **Send SMS Code**
   ```
   POST /chapi/api/jwt_send_msg
   Authorization: Bearer <jwt_token>
   Content-Type: application/json
   Body: {}

   Response: { "code": 20000, "data": {}, "message": "发送成功", "success": true }
   ```

2. **Verify SMS Code** (submit verification form on the web page)

---

## Common Request Headers

All authenticated API requests require:

```
Authorization: Bearer <jwt_token>
X-XSRF-TOKEN: <xsrf_token>
UUID: Web_PKUHOLE_2.0.0_WEB_UUID_<device_uuid>
Content-Type: application/json
Cookie: pku_token=<jwt>; pku_expires_in=<ts>; pku_uid=<uid>; XSRF-TOKEN=<xsrf>; _session=<session>
```

## Response Format

All API responses follow this format:
```json
{
  "code": 20000,
  "data": { ... },
  "message": "success",
  "success": true,
  "timestamp": 1775473186
}
```

Error codes:
- `20000`: Success
- `40002`: SMS verification required

---

## API Endpoints

### Holes (Posts)

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/hole/list_comments?page=1&limit=10&comment_limit=10&comment_stream=1` | List holes with comments (main feed) |
| GET | `/hole/one?pid=<pid>&comment_stream=1` | Get single hole with all comments |
| GET | `/hole/list?page=1&limit=10` | List holes (simple) |
| GET | `/hole/my_list?page=1&limit=10` | List my holes |
| GET | `/hole/ta_list` | List a user's holes |
| GET | `/hole/history` | Hole history |
| DELETE | `/hole/history_del` | Delete hole history |
| POST | `/hole/post` | Create new hole |
| POST | `/hole/attention` | Follow a hole |
| POST | `/hole/attention_cancel` | Unfollow a hole |
| POST | `/hole/attention_update` | Update follow settings |
| POST | `/hole/praise` | Like/praise a hole |
| POST | `/hole/tread` | Dislike/tread a hole |
| POST | `/hole/fold` | Fold a hole |
| POST | `/hole/report` | Report a hole |
| GET | `/hole/get` | Get hole by ID |

### Comments

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/comment/list?pid=<pid>&page=1&limit=30` | List comments for a hole |
| GET | `/comment/get` | Get single comment |
| POST | `/comment/post` | Post a comment |
| POST | `/comment/good` | Mark comment as good/reward |
| POST | `/comment/report` | Report a comment |

### Bookmarks (Favorites)

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/bookmark/list?page=1&limit=60` | List bookmarks |
| GET | `/bookmark/get` | Get bookmark |
| POST | `/bookmark/add` | Add bookmark |
| POST | `/bookmark/del` | Remove bookmark |
| POST | `/bookmark/update` | Update bookmark |

### Messages

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/message/index?page=1&limit=20` | List messages |
| GET | `/message/un_read?message_type=int_msg` | Get unread internal message count |
| GET | `/message/un_read?message_type=sys_msg` | Get unread system message count |
| POST | `/message/set_read` | Mark messages as read |
| POST | `/message/set` | Update message settings |
| POST | `/message/del` | Delete message |

### Users

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/users/info` | Get current user info (body: `{}`) |

### Media

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/media/upload` | Upload image |
| GET | `/media/get?id=<id>` | Get media |
| GET | `/media/getThumbnail?id=<id>` | Get media thumbnail |

### Tags

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/tags/tree` | Get tag tree (categories + tags) |
| GET | `/tags/list` | List tags |

### Search

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/search?keyword=<keyword>&page=1&limit=10` | Search holes by keyword or #PID |

### User Config

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/user_config/get?type=<type>` | Get user config (type=2: settings, type=3: other) |
| POST | `/user_config/update` | Update user config |

### Exclusive ID

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/exclusive_id/list` | List exclusive IDs |

### Draft

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/draft/list` | List drafts |
| POST | `/draft/save` | Save draft |
| POST | `/draft/del` | Delete draft |

### Reminders

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/reminder/list?page=1&limit=1000` | List reminders |

### Navigation

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/navigation-items/list?page=1&limit=1000` | List navigation items |

### Person Blocking

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/person_blocking_words/index` | List blocked words |

### Reports

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/report/contents` | Get report content options |

### Other

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/chapi/version?t=<timestamp>` | Get API version (non-v3) |
| POST | `/chapi/api/jwt_send_msg` | Send SMS verification code (non-v3) |
| POST | `/chapi/api/jwt_msg_verify` | Submit SMS verification code (non-v3, body: `{"valid_code": "..."}`) |
| POST | `/chapi/api/check_otp` | Check OTP dynamic token (non-v3) |
| POST | `/chapi/api/login_iaaa_check_token` | Check IAAA login token (non-v3) |

---

## Data Models

### Hole (Post)
```json
{
  "pid": 8132392,
  "text": "content text",
  "type": "text",
  "timestamp": 1775469368,
  "hidden": 0,
  "reply": 29,
  "likenum": 5,
  "extra": 0,
  "anonymous": 1,
  "tag": null,
  "is_top": 0,
  "is_comment": 1,
  "tags_ids": "",
  "media_ids": "",
  "fold": 0,
  "reward_cost": 0,
  "reward_state": 0,
  "identity_show": 0,
  "is_follow": 0,
  "is_praise": 0,
  "is_tread": 0,
  "tread_num": 0,
  "praise_num": 0
}
```

### Comment
```json
{
  "cid": 37426925,
  "pid": 8132392,
  "text": "comment content",
  "timestamp": 1775469860,
  "hidden": 0,
  "anonymous": 1,
  "comment_id": null,
  "name_tag": "Alice",
  "media_ids": "",
  "is_lz": 0,
  "quote": {
    "cid": 37426971,
    "text": "quoted comment",
    "name_tag": "Bob"
  }
}
```

### User Info
```json
{
  "uid": "<student_id>",
  "name": "<real_name>",
  "pkuhole_push": 1,
  "newmsgcount": 1,
  "action_remaining": 50,
  "leaf_balance": 100,
  "is_black": 0
}
```
