# Medplum OAuth Redirect Configuration Issue

## Problem

The Medplum server at `http://10.241.15.154:8103` is redirecting OAuth requests to `localhost:3000` instead of the external IP, breaking authentication for clients on other machines.

## Current Behavior

When a client initiates OAuth:

```
GET http://10.241.15.154:8103/oauth2/authorize?...
```

The server responds with:

```
HTTP/1.1 302 Found
Location: http://localhost:3000/oauth?...
```

This redirect to `localhost:3000` fails for any client not running on the Medplum server machine itself.

## Expected Behavior

The redirect should use the external IP:

```
HTTP/1.1 302 Found
Location: http://10.241.15.154:3000/oauth?...
```

## Required Fix

Update the Medplum server configuration to use the external base URL for the frontend application.

### If using medplum.config.json:

```json
{
  "baseUrl": "http://10.241.15.154:8103/",
  "appBaseUrl": "http://10.241.15.154:3000/"
}
```

### If using environment variables:

```bash
MEDPLUM_BASE_URL=http://10.241.15.154:8103/
MEDPLUM_APP_BASE_URL=http://10.241.15.154:3000/
```

### If using Docker Compose:

```yaml
services:
  medplum-server:
    environment:
      - MEDPLUM_BASE_URL=http://10.241.15.154:8103/
      - MEDPLUM_APP_BASE_URL=http://10.241.15.154:3000/
```

## Verification

After updating, test with:

```bash
curl -s -I "http://10.241.15.154:8103/oauth2/authorize?response_type=code&client_id=af1464aa-e00c-4940-a32e-18d878b7911c&redirect_uri=fabricscribe%3A%2F%2Foauth%2Fcallback&scope=openid&code_challenge=test&code_challenge_method=S256&state=test" | grep Location
```

Should return:
```
Location: http://10.241.15.154:3000/oauth?...
```

## Client Details

- **Client ID**: `af1464aa-e00c-4940-a32e-18d878b7911c`
- **Redirect URI**: `fabricscribe://oauth/callback`
- **Scopes**: `openid profile`
