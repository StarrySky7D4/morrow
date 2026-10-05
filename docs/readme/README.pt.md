# Morrow · 明隙

[简体中文（默认）](../../README.md) · [English](../../docs/readme/README.en.md) · [Русский](../../docs/readme/README.ru.md) · [Français](../../docs/readme/README.fr.md) · [Deutsch](../../docs/readme/README.de.md) · [Español](../../docs/readme/README.es.md) · [日本語](../../docs/readme/README.ja.md) · [한국어](../../docs/readme/README.ko.md) · **Português**

Deixe um pouco de espaço para as ideias de amanhã.

Morrow (明隙) é um espaço de trabalho local baseado em cartões, em evolução para uma arquitetura de plugins multiplataforma. Antes chamado daemon, seu pacote de código é `morrow_studio`. No Windows, o Flutter fornece a interface e um host Rust com plugins Wasm isolados executa a lógica. A versão Web executa Rust/Wasm localmente no navegador; Android ainda usa o caminho anterior. A equivalência completa do produto e dos plugins entre plataformas ainda não foi validada.

## Estado do desenvolvimento (2026-10-05)

O ponto de controle do código da aplicação é **0.1.9-test.58+62**. A linha de desenvolvimento prevista é `codex/windows-sdk-convergence-20261005`, baseada em cloud `468ef2e` e nas alterações C02–C07. O [estado do projeto](../../docs/PROJECT_STATUS.md) centraliza a situação e os próximos passos; o [relatório C07 de escopo limitado](../../reports/reconstruction-2026-10-05/directory-owner-sdk.md) registra as evidências reais de Windows.

A validação local de Windows com dados sintéticos passou: 115 métodos do caminho owner original, incluindo 7 de cancelamento de diretório, 14 de directory owner e 16 de diretório nativo; 42 métodos de regressão originais; 100 métodos de rede em 11 programas. A execução dos 42 métodos mantém 14 filtros frozen-region e um remote-reader. Os auxiliares filhos não são métodos aprovados adicionais. Esses números não representam aceitação do SDK completo ou do produto.

As operações confiáveis de diretório capture/page/finish usam a fila IoWorker, Manager, runtime, instância, IoBinding e aprovação FileList originais. Amostragem do relógio e verificação de autorização usam etapas atômicas curtas; a quota só é liberada após a liberação efetiva dos recursos nativos. Resultados Unknown não são repetidos automaticamente e os cursores não voltam ao início. Bibliotecas de payloads tipados WebSocket/SSE, com versões independentes, oferecem codecs Rust, C e C++ e validação guest limitada; codecs de diretório/blob e estado limitado também possuem validação local.

**SDK26/G04 continua OPEN; o SDK completo não está congelado.** Owners de produção protegidos, entrada de tarefas GUI/espaço de trabalho, seletor e proveniência dos ancestrais, fábrica de segredos confiáveis novos, negociação de novos request/import/feature/helper profile de diretório, histórico durável de blob, qualificação completa do produto de serviços de entrada, TLS/API reais e outras plataformas continuam pendentes ou NOT_RUN. C08 é apenas projeto. Esta validação não publica um novo instalador nem constitui um SDK estável.

## Download e compatibilidade

Última prévia publicada da aplicação: **0.1.9-test.56+60**, uma **prévia de teste para Windows x64**, não estável. Baixe o ZIP do Windows, o ZIP do código-fonte correspondente e a lista SHA-256. Extraia tudo e execute `morrow_studio.exe`, mantendo as DLLs, o host, `data`, `plugins` e os arquivos de licença.

[Baixar test.56](https://github.com/StarrySky7D4/morrow/releases/tag/v0.1.9-test.56) · [Versão compatível test.1](https://github.com/StarrySky7D4/morrow/releases/tag/v0.1.9-test.1)

`test.1` é a última versão de teste `0.1.x` compatível com os tipos de dados originais. As versões seguintes avançam a reescrita e podem trazer alterações incompatíveis. `0.2.0` virá após a estabilização e validação da arquitetura e do modelo de dados. Os dados do test.1 não são importados nem sobrescritos automaticamente. Antes de atualizar, faça uma cópia da biblioteca e do arquivo de proteção original. A proteção é vinculada ao usuário do Windows; copiar apenas o banco não permite migrar entre contas.

## Recursos

- Cartões: ideias, projetos, experimentos, favoritos, pesquisa, listas e desfazer exclusões; edição Markdown com prévia e cópias independentes dos anexos.
- Captura de conteúdo: texto, capturas de tela, arquivos, texto formatado e tabelas do Office. Formatos complexos ou proprietários podem ser preservados como prévias ou anexos; o layout original não é garantido.
- Aparência: vidro fosco, ultratransparente e líquido; fundos padrão, sólidos, texturizados ou transparentes; roda de cores e ajustes por cartão ou componente, com movimento reduzido.
- Mídia: fundos de imagem, GIF e vídeo, música local, letras e dicas flutuantes. Os formatos dependem da plataforma e do decodificador. A transparência Web mostra a página hospedeira, não a área de trabalho.
- Layout e idiomas: conteúdo responsivo e páginas de configurações separadas; chinês, inglês, russo, francês, alemão, espanhol, japonês, coreano e português.
- Proteção no Windows: armazenamento unificado em Rust, selagem dos registros de auditoria, snapshots, backup e recuperação da identidade original e uso simultâneo limitado da mesma identidade.

## Mudanças e validação da versão publicada test.56

test.56 adiciona ações de contexto, ordenação e arraste de cartões, edição individual de dicas e plugins de tema independentes. Melhora o layout em colunas, a atualização dos filtros e o encerramento. O salvamento automático completo e a equivalência entre plataformas continuam pendentes.

### Alterações anteriores de test.55

test.55 adiciona sete estilos visuais e ajuste de profundidade, melhora as animações dos controles e o espaçamento entre componentes, corrige o salvamento das configurações e a aplicação das fontes, melhora o encerramento seguro em segundo plano e estabelece a base da recuperação de rascunhos. O salvamento automático na interface principal ainda não está concluído.

### Otimização e validação anteriores de test.54

test.54 elimina verificações completas repetidas ao abrir bibliotecas, decodifica evidências com paralelismo limitado e reutiliza tamanhos e resumos de arquivos verificados na mesma transação. Cada abertura continua com verificação completa. O formato de armazenamento e o plugin incluído não mudam; o cache de validação não persiste entre execuções.

A última otimização de leitura foi comparada com a versão paralela anterior em quatro inicializações alternadas por versão. Na mesma máquina, com uma cópia de biblioteca de cerca de 100 MB, a mediana até remover a tela de carregamento caiu de 2.222 para 1.299 s desde a entrada do Dart. Registrar a cópia pode aquecer o cache de arquivos; não é garantia de inicialização a frio com cache vazio. Passaram 568 testes Core, 97 Audit e 2 de integração com host real; um teste Core preexistente foi ignorado. Consulte as condições nas [notas de test.54](../../reports/0.1.9-test.54-release.md).

## Executar a partir do código-fonte

Requer Flutter 3.44 / Dart 3.12 ou versões compatíveis. O Windows também precisa das ferramentas C++ para desktop do Visual Studio, Windows SDK, Rust e compilador Cap’n Proto no PATH.

Compilação completa do ambiente Rust Windows, testes de integração e empacotamento:

```powershell
flutter pub get
rustup target add wasm32-unknown-unknown
pwsh -File tool/build_rust_workbench_windows.ps1
```

Os artefatos não podem ser sobrescritos: use uma nova versão ou um novo diretório de saída. `-RefreshArtifact` não permite mais substituição. Distribua todo o diretório de execução. A documentação das plataformas define a validação Web/Android; compilar para Windows não a substitui.

Desenvolvimento e verificações básicas:

```powershell
flutter run -d windows
flutter run -d chrome
flutter analyze
flutter test
flutter build web --no-web-resources-cdn
```

## Arquitetura, SDK e próximos passos

A arquitetura pretendida reúne interface Flutter/Dart, núcleo Rust portátil e ambientes de execução de plugins substituíveis. As fronteiras de execução usam contratos fixos; persistência e intercâmbios próprios usam Protobuf + LZ4. Os SDKs C/C++/Rust e a interface declarativa de plugins estão em desenvolvimento. Plugins TS/JS não são suportados; plugins Dart dinâmicos não são necessários.

O SDK completo ainda não está congelado. Requisições HTTP/HTTPS gerenciadas, nós de serviço API limitados e gerenciamento de identidades TLS estão conectados. Faltam a conciliação de resultados Unknown após reinícios, o sistema de arquivos completo, a negociação de novos request/import/feature/helper profile de diretório e a validação multiplataforma. A abertura ainda percorre todo o histórico; não há pipeline completo de leitura/cálculo, organização do histórico em camadas nem limpeza automática.

## Documentação

- [Estado atual do projeto](../../docs/PROJECT_STATUS.md)
- [Notas da versão e validação](../../reports/0.1.9-test.56-release.md)
- [Quadro de desenvolvimento](../../docs/DEVELOPMENT_BOARD.md)
- [Roteiro de arquitetura](../../docs/FUTURE_ROADMAP.md)
- [Migração de recursos e limites](../../docs/TEST1_RUST_PARITY.md)
- [SDK C/C++/Rust](../../sdk/README.md)
- [Interface de plugins](../../docs/PLUGIN_SDK_AND_UI.md)
- [Captura e formatos](../../docs/RICH_CAPTURE.md)
- [Compilação Android](../../docs/ANDROID.md)
- [Compatibilidade da renomeação](../../docs/RENAMING.md)
- [README histórico e marcos](../../docs/history/README-before-test54.md)

Os projetos detalhados e relatórios estão principalmente em chinês. Todos os READMEs apresentam a mesma versão atual; as traduções ainda não passaram por revisão abrangente de falantes nativos.

## Licença

A partir de `0.1.9-test.2`, código próprio, SDKs, documentação, configurações e recursos usam **AGPL-3.0-only**. test.1 e versões anteriores mantêm Apache-2.0; conteúdos de terceiros mantêm suas licenças. Os pacotes incluem licenças, avisos de direitos autorais e acesso ao código-fonte correspondente.

[AGPL-3.0-only](../../LICENSE) · [NOTICE](../../NOTICE) · [Third-party licenses](../../packaging/THIRD_PARTY_NOTICES.txt)
