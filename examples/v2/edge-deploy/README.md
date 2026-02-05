# Edge Deployment Example

Configuration template for deploying WASI components to edge platforms.

## Files

- `run.toml` - Deployment configuration for Cloudflare, AWS Lambda, Vercel

## Supported Providers

- Cloudflare Workers
- AWS Lambda
- Vercel
- Fastly Compute@Edge

## Usage

1. Build your HTTP handler component
2. Update `run.toml` with your component path
3. Set environment variables for your provider
4. Run `run v2 deploy --target edge --provider cloudflare`

## Deploy Commands

```bash
# Cloudflare
export CLOUDFLARE_API_TOKEN=your_token
run v2 deploy --target edge --provider cloudflare

# AWS Lambda
export AWS_ACCESS_KEY_ID=your_key
export AWS_SECRET_ACCESS_KEY=your_secret
run v2 deploy --target edge --provider aws-lambda

# Vercel
export VERCEL_TOKEN=your_token
run v2 deploy --target edge --provider vercel
```
