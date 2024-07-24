# 🛰️ Put your Laravel sites in Orbit.

> Orbit provides a simple way to deploy your Laravel sites to your server in a few seconds.

## Get Started

1. Get the Orbit server up and running on your VPS. You'll need a [GitHub token](https://github.com/settings/personal-access-tokens/new) and the [server binary](https://github.com/m1guelpf/orbit/releases/latest) (there's a [Docker image](https://github.com/m1guelpf/orbit/pkgs/container/orbit-server) too!).
    > The Orbit server exposes an HTTP API, which you'll need to make accessible to the outside world.
2. Create an `Orbit.toml` config file and add your sites to it, like so:

```toml
version = 1
token = ""  # Use `openssl rand -base64 32` to generate a random token

[[sites]]
name = "Test Site"
path = "/var/www/test-site"
github_repo = "m1guelpf/laravel-test"
commands = [ # Extra commands to run during the deployment (optional)
    "php horizon:terminate"
]
```

3. Create a `.github/workflows/deploy.yaml` GitHub action, like so:

```yaml
name: Deploy to prod
on:
    push:
        branches: [main]

jobs:
    deploy:
        runs-on: ubuntu-latest
        steps:
            - name: Deploy to prod
              uses: m1guelpf/orbit@main
              with:
                  site: alexandria # slug of your site, generated from the name above
                  orbit-url: ${{ secrets.ORBIT_URL }} # URL to your Orbit instance
                  orbit-token: ${{ secrets.ORBIT_TOKEN }} # The token you generated on your Orbit config
```

4. That's it! Pushing to `main` will now deploy your site, with no downtime for your users 🎉

## Architecture

### 🌐 Orbit Server

The Orbit Server is the main component of the system. It exposes an HTTP API that lets you trigger deployments and streams the results back in real time. It's the module that actually contains the logic for deploying the sites.

### 🌠 Orbit Client

To make interacting with the Server easier, the Orbit Client provides a simple Rust interface for calling the API, dealing with serialization and such. If you want to write your own Orbit integration, it'll let you interact with the Server as if it was just another crate.

### 🐙 Orbit CLI & GitHub Action

To make deployments easier for end users, Orbit also includes a CLI that provides user-friendly live output for deployments. And, if you want to run it from GitHub Actions, it comes packaged into its own GitHub Action, making zero-downtime deployments a one-line change.

## License

This project is licensed under the MIT License - see the [LICENSE file](LICENSE) for details.
