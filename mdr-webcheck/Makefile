localhost:
	source .env && cargo run -- --admin-token $$ADMIN_SECRET_KEY -u localhost

staging:
	source .env && cargo run -- --admin-token $$ADMIN_SECRET_KEY -u staging.mdrepo.org

prod:
	source .env && cargo run -- --admin-token $$PROD_ADMIN_SECRET_KEY -u mdrepo.org
