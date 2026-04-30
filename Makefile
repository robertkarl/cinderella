
llama=/opt/homebrew/bin/llama-server
modelcachedir=/Users/robertkarl/Library/Caches/llama.cpp
9boblit=mradermacher_Huihui-Qwen3.5-9B-abliterated-GGUF_Huihui-Qwen3.5-9B-abliterated.Q4_K_M.gguf

startllama_9boblit_ctx16k:
	${llama} --model  "${modelcachedir}/${9boblit}" --host localhost --port 8081 --ctx-size 16384 --jinja 

startllama_9boblit_32k:
	${llama} --model  "${modelcachedir}/${9boblit}" --host localhost --port 8081 --ctx-size 32768 --jinja 

stop_all_llama_server:
	killall llama-server

edit_pi_configs:
	vim /Users/robertkarl/.pi/agent/models.json /Users/robertkarl/.pi/agent/settings.json -p

pi-read-only:
	pi --tools read,grep,find,ls

# Run cinderella against an already-running llama-server on port 8081 (no server startup)
cindy-no-start-llama:
	cargo run -- --api-url http://localhost:8081 .

# Run cinderella against the homelab 35B MoE (llama-swap on VM 220)
cindy-remote:
	cargo run -- --api-url http://192.168.50.4:11434 --model-name "qwen3.5:35b-a3b-coding" .

remote-query-models:
	curl http://192.168.50.4:11434/v1/models

test:
	cargo test

deploy:
	@echo "No deploy target configured. Cinderella is a local CLI tool."

smoke-test:
	@echo "No smoke test configured. Cinderella is a local CLI tool."

.PHONY: stop_all_llama_server startllama pidev cindy-no-start-llama cindy-remote remote-query-models test deploy smoke-test
