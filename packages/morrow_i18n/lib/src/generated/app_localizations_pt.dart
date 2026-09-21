// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Portuguese (`pt`).
class AppLocalizationsPt extends AppLocalizations {
  AppLocalizationsPt([String locale = 'pt']) : super(locale);

  @override
  String get commonAppName => 'Morrow';

  @override
  String get commonCancel => 'Cancelar';

  @override
  String commonCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count itens',
      one: '$count item',
      zero: 'Nenhum item',
    );
    return '$_temp0';
  }

  @override
  String commonGreeting(String name) {
    return 'Olá, $name';
  }

  @override
  String get importsAttachmentLimit =>
      'Importe até 20 anexos por vez. Cole o restante separadamente.';

  @override
  String get importsClipboardChanged =>
      'A área de transferência mudou durante a leitura. Cole novamente.';

  @override
  String get importsEmbeddedImageUnreadable =>
      'Não foi possível ler uma imagem incorporada.';

  @override
  String get importsEmbeddedImagesSeparate =>
      'Algumas imagens incorporadas devem ser importadas separadamente.';

  @override
  String get importsExcelValues =>
      'Valores e fórmulas convertidos para Markdown. A formatação e as células mescladas estão no anexo XML.';

  @override
  String get importsExcelXmlKept =>
      'A tabela original do Excel foi preservada como anexo XML.';

  @override
  String get importsFileTooLarge =>
      'O arquivo da área de transferência excede 200 MB.';

  @override
  String get importsItemLimit =>
      'Apenas os primeiros 20 itens foram lidos. Cole os demais separadamente.';

  @override
  String get importsItemUnreadable =>
      'Não foi possível ler um item da área de transferência. O restante legível foi preservado.';

  @override
  String get importsLocalImageNotRead =>
      'Imagens locais vinculadas não são lidas automaticamente. Cole a imagem ou importe o arquivo original.';

  @override
  String get importsMergedTable =>
      'Células mescladas convertidas em tabela legível. A formatação original está no anexo HTML.';

  @override
  String get importsOfficeBusy =>
      'Outro aplicativo está usando a área de transferência. Os objetos do Office não foram lidos.';

  @override
  String get importsOfficeEmbeddedKept =>
      'O objeto do Office foi preservado como anexo original. Edite gráficos, fórmulas e layout no aplicativo original.';

  @override
  String get importsOfficeExportFailed =>
      'O objeto do Office excede o limite ou não pôde ser exportado. Salve no aplicativo original e importe.';

  @override
  String get importsOfficeReadFailed =>
      'Não foi possível ler o conteúdo do Office. O restante da área de transferência continua disponível.';

  @override
  String get importsOfficeUnavailable =>
      'A área de transferência do Office está temporariamente indisponível.';

  @override
  String get importsOfficeUnreadable =>
      'Não foi possível ler o objeto original do Office. O restante disponível foi preservado.';

  @override
  String get importsRichFallback =>
      'Parte da formatação não foi convertida. O texto legível foi preservado.';

  @override
  String get importsRichTooLarge =>
      'O texto formatado excede 2 MB. Importe o documento como anexo.';

  @override
  String get importsRtfTooLarge =>
      'O conteúdo RTF é muito grande. Importe o documento original.';

  @override
  String get importsSpreadsheetTooLarge =>
      'A tabela é muito grande. Importe o arquivo do Excel.';

  @override
  String get importsTableConverted =>
      'Tabela convertida para Markdown. Os dados completos estão no anexo TSV.';

  @override
  String get importsTextTooLarge =>
      'O texto excede 2 MB. Importe como arquivo.';

  @override
  String get importsTotalTooLarge =>
      'Os arquivos colados excedem 200 MB no total. Importe em grupos menores.';

  @override
  String get importsUnsupported =>
      'O acesso à área de transferência não é suportado aqui. Importe um arquivo.';

  @override
  String get mainActiveProjects => 'Em andamento';

  @override
  String get mainAdjustCustomTone => 'Ajustar tonalidade';

  @override
  String get mainAmbientDetail => 'A luz fluida adiciona um toque de cor.';

  @override
  String get mainAppTitle => 'Morrow — Espaço para ideias';

  @override
  String get mainAppearance => 'Aparência';

  @override
  String get mainArrangeIdeas => 'Ordenar ideias';

  @override
  String mainAttachmentCount(int count) {
    return 'Anexos · $count';
  }

  @override
  String mainAttachmentHint(int count, String name) {
    return '$count anexos · $name';
  }

  @override
  String get mainAttachmentLimit => 'Até 20 anexos por registro.';

  @override
  String get mainAutosaveNotice =>
      'A aparência e as ideias são salvas localmente';

  @override
  String get mainAwaitDiscovery => 'À espera de uma descoberta';

  @override
  String get mainBackToWorkbench => 'Voltar ao espaço de trabalho';

  @override
  String get mainBackgroundCanvas => 'Tela de fundo';

  @override
  String get mainBackgroundSound => 'Reproduzir áudio de fundo';

  @override
  String get mainBodyHint =>
      'Escreva suas ideias ou cole conteúdo…\n\nSuporta # títulos, listas, tabelas e blocos de código';

  @override
  String get mainBrightWhite => 'Branco';

  @override
  String get mainBuiltinTexture => 'Usar textura integrada';

  @override
  String get mainCanvasCompass => 'Círculo de cores do fundo';

  @override
  String get mainCaptureIdea => 'Registrar ideia';

  @override
  String get mainCaptureNow => 'Anotar uma ideia';

  @override
  String mainCardAttachments(int count, String name) {
    return 'Arquivos $count · $name';
  }

  @override
  String get mainCategoryExperiment => 'Experimento';

  @override
  String get mainCategoryIdea => 'Ideia';

  @override
  String get mainCategoryProject => 'Projeto';

  @override
  String get mainCategoryPrompt => 'Onde guardar';

  @override
  String get mainChangeFailed =>
      'A alteração não foi salva. O rascunho foi preservado; tente novamente.';

  @override
  String get mainCheckAgain => 'Verificar novamente';

  @override
  String get mainClearSearch => 'Limpar pesquisa';

  @override
  String get mainClipboardEmpty =>
      'Não há texto ou arquivos legíveis na área de transferência. Copie um arquivo no gerenciador ou use Importar arquivo.';

  @override
  String get mainClipboardReadFailed =>
      'Não foi possível ler o conteúdo. Importe um arquivo ou verifique as permissões do arquivo e da área de transferência.';

  @override
  String get mainClipboardSupport =>
      'Suporta Markdown, texto formatado e tabelas do Office, capturas e arquivos. Objetos complexos mantêm os anexos originais. Até 20 anexos de 200 MB cada.';

  @override
  String get mainCollapseSidebar => 'Recolher barra lateral';

  @override
  String get mainCompletedProjects => 'Concluídos';

  @override
  String get mainComponentCompass => 'Círculo de cores do componente';

  @override
  String get mainComponentEmpty => 'Estado vazio';

  @override
  String get mainComponentFooter => 'Dicas e letras no rodapé';

  @override
  String get mainComponentHero => 'Cartão de visão geral';

  @override
  String get mainComponentNavigation => 'Navegação lateral';

  @override
  String get mainComponentQuickCapture => 'Nota rápida';

  @override
  String get mainComponentSearch => 'Barra de pesquisa';

  @override
  String get mainComponentSettings => 'Componentes e cartões · Configurações';

  @override
  String get mainContentProtection => 'Proteção do conteúdo';

  @override
  String get mainContentRead => 'Conteúdo lido';

  @override
  String mainContentReadFiles(int count) {
    return 'Conteúdo lido; $count anexos preservados';
  }

  @override
  String get mainCornerRadius => 'Raio dos cantos';

  @override
  String get mainCredentialSettings => 'Credenciais';

  @override
  String get mainCrystal => 'Cristalino';

  @override
  String get mainCrystalDetail =>
      'Leve e transparente, deixando as cores brilharem.';

  @override
  String get mainCuriosity =>
      'Coisas interessantes começam\ncom um pouco de curiosidade.';

  @override
  String get mainCustomCompass => 'Círculo de cores · Personalizado';

  @override
  String get mainCustomLightness => 'Luminosidade personalizada';

  @override
  String get mainCustomTheme => 'Personalizado';

  @override
  String get mainDaily => 'Pequenas coisas';

  @override
  String get mainDailyExplore => 'Reserve dez minutos para explorar';

  @override
  String get mainDailyIdea => 'Anote uma ideia';

  @override
  String get mainDailyWater => 'Sirva-se um copo de água';

  @override
  String get mainDarkTheme => 'Escuro';

  @override
  String get mainDeepBlack => 'Preto';

  @override
  String get mainDefaultCanvas => 'Padrão';

  @override
  String get mainDefaultGlobalColor =>
      'Cor padrão do tema · Todos os controles';

  @override
  String get mainDelete => 'Excluir';

  @override
  String mainDeleted(String title) {
    return '“$title” excluído';
  }

  @override
  String get mainDiagnosticDetails => 'Detalhes do diagnóstico';

  @override
  String get mainDone => 'Concluído';

  @override
  String get mainEdit => 'Editar';

  @override
  String get mainEditIdeaTitle => 'Dê clareza à sua ideia';

  @override
  String get mainEditorClosedUnknown =>
      'O editor fechou, mas o salvamento não foi confirmado. Reabra o espaço de trabalho e verifique antes de criar outra cópia.';

  @override
  String get mainEditorSubtitle =>
      'Textos, tabelas, imagens: guarde tudo aqui enquanto a ideia ganha forma.';

  @override
  String get mainEditorUnavailable =>
      'Editor indisponível. Verifique o serviço de conteúdo e tente novamente.';

  @override
  String get mainEndpointSettings => 'Endpoints de saída';

  @override
  String get mainExpandSettings => 'Expandir configurações';

  @override
  String get mainExpandSidebar => 'Expandir barra lateral';

  @override
  String get mainExtensionPlugins => 'Extensões';

  @override
  String get mainFavoriteAttachments => 'Anexos favoritos';

  @override
  String get mainFavoriteRecords => 'Registros favoritos';

  @override
  String mainFavoriteTooltip(String title) {
    return 'Adicionar $title aos favoritos';
  }

  @override
  String get mainFavoritesIntro =>
      'Seus textos, imagens e arquivos favoritos em um só lugar.';

  @override
  String mainFieldLimit(int limit) {
    return 'Máximo de $limit caracteres. Reduza o conteúdo ou importe como arquivo.';
  }

  @override
  String get mainFilterAll => 'Todos';

  @override
  String get mainFilterAttachments => 'Com arquivos';

  @override
  String get mainFilterFavorites => 'Apenas favoritos';

  @override
  String get mainFilterFile => 'Arquivos';

  @override
  String get mainFilterImage => 'Imagens';

  @override
  String get mainFilterMedia => 'Áudio / vídeo';

  @override
  String get mainFilterPending => 'A fazer';

  @override
  String get mainFilterText => 'Texto';

  @override
  String get mainFollowTheme => 'Seguir tema';

  @override
  String get mainFrostDetail => 'Suavize o fundo e dê espaço aos pensamentos.';

  @override
  String get mainFrostEffect => 'Desfoque';

  @override
  String get mainFrostOpacity => 'Opacidade do vidro';

  @override
  String get mainFrostUnavailable =>
      'O desfoque da área de trabalho está indisponível. A tonalidade e a opacidade continuam ajustáveis.';

  @override
  String get mainFrosted => 'Fosco';

  @override
  String get mainGlassTexture => 'Estilo de vidro';

  @override
  String mainGlobalColor(String color) {
    return '$color · Todos os controles';
  }

  @override
  String get mainGreeting => 'Deixe suas ideias crescerem.';

  @override
  String get mainGreetingDetail =>
      'Guarde os detalhes do dia a dia e as faíscas de inspiração.';

  @override
  String get mainHeroBody =>
      'Um pensamento, uma tarefa, um «e se».\nTudo começa aqui.';

  @override
  String get mainHeroCaption => 'O CANTO DAS POSSIBILIDADES';

  @override
  String get mainHeroTitle => 'Tudo bem começar aos poucos.';

  @override
  String get mainHideAppearance => 'Ocultar ajustes de aparência';

  @override
  String get mainHideCustomTone => 'Ocultar tonalidade personalizada';

  @override
  String get mainHidePreview => 'Ocultar prévia';

  @override
  String get mainHttpSettings => 'Tarefas HTTP';

  @override
  String get mainHypothesis => 'Hipótese';

  @override
  String get mainHypothesisPrompt => 'Hipótese a testar';

  @override
  String get mainHypothesisSection => 'Hipótese / O que tentar';

  @override
  String get mainIdeaDetails =>
      'Guarde os detalhes. Deixe o próximo passo mais claro.';

  @override
  String get mainIdeaNameHint => 'Dê um nome';

  @override
  String get mainIdeaNameRequired => 'Escreva sua ideia primeiro';

  @override
  String get mainIdeaSaved => 'Ideia salva.';

  @override
  String get mainImportFailed =>
      'Não foi possível importar a mídia. Verifique o arquivo e o espaço disponível.';

  @override
  String get mainImportFile => 'Importar arquivo';

  @override
  String get mainInboxIntro =>
      'Registre primeiro, organize depois. Transforme boas ideias em pequenos projetos.';

  @override
  String get mainIoNoDeclarations =>
      'Nenhum plugin instalado declara acesso a arquivos ou rede.';

  @override
  String get mainIoSettings => 'Rede e arquivos';

  @override
  String get mainIoSettingsGuide =>
      'Gerencie permissões de arquivos e rede separadamente da aparência. Aprovar uma capacidade não libera todos os arquivos ou endpoints; as operações dependem do backend atual.';

  @override
  String get mainIoSettingsSummary =>
      'Permissões, credenciais, endpoints e serviços de API';

  @override
  String get mainJustNow => 'Agora mesmo';

  @override
  String get mainLabIntro =>
      'Comece com uma hipótese. Guarde tentativas, observações e surpresas.';

  @override
  String get mainLanguage => 'Idioma';

  @override
  String get mainLanguageChinese => '简体中文';

  @override
  String get mainLanguageEnglish => 'English';

  @override
  String get mainLanguageSystem => 'Idioma do sistema';

  @override
  String get mainLavender => 'Lavanda';

  @override
  String get mainLightOpacity => '20% · Leve';

  @override
  String get mainLiquidAllCanvases =>
      'Disponível separadamente nos quatro tipos de fundo';

  @override
  String get mainLiquidDetail =>
      'Reflexos fluidos e refração suave, como uma gota de água suspensa.';

  @override
  String get mainLiquidEffect => 'Efeito de vidro líquido';

  @override
  String get mainLiquidGlass => 'Vidro líquido';

  @override
  String get mainLivePreview => 'Prévia ao vivo';

  @override
  String get mainLocalMedia => 'Mídia local';

  @override
  String get mainMakeYours => 'DO SEU JEITO';

  @override
  String get mainMarkOrganized => 'Marcar como organizado';

  @override
  String get mainMarkdownBody => 'Texto · Markdown';

  @override
  String get mainMediaLimits => 'Imagens / GIFs ≤ 25 MB; vídeos ≤ 150 MB';

  @override
  String get mainMonochrome => 'Monocromático';

  @override
  String mainMoreSteps(int count) {
    return 'Mais $count etapas; abra para ver';
  }

  @override
  String mainMovedProject(String title) {
    return '“$title” movido para Projetos';
  }

  @override
  String get mainMusic => 'Reprodutor de música';

  @override
  String get mainMySpace => 'Meu espaço';

  @override
  String get mainNavigation => 'Navegação';

  @override
  String get mainNewIdea => 'Nova ideia';

  @override
  String get mainNewIdeaTitle => 'Capture uma nova ideia';

  @override
  String get mainNoHypothesis => 'Ainda não há hipótese';

  @override
  String get mainNoMatches => 'Nenhuma ideia correspondente';

  @override
  String get mainNoResultYet =>
      'O resultado pode esperar. O processo também merece ser registrado.';

  @override
  String get mainNotNow => 'Agora não';

  @override
  String get mainObservationSection => 'Observações / Descobertas';

  @override
  String get mainObservations => 'Observações e conclusões';

  @override
  String get mainObservationsPrompt => 'Observações, processo e conclusões';

  @override
  String get mainOneHourAgo => 'Há 1 hora';

  @override
  String get mainOnlineMedia => 'Mídia online';

  @override
  String get mainOpaqueFallback =>
      'Painéis transparentes sobre a cor do tema atual.';

  @override
  String get mainOpenNextStep =>
      'Abra o projeto para editar os próximos passos';

  @override
  String get mainOrganizedCount => 'Organizados';

  @override
  String get mainOriginalColors => 'Cores originais';

  @override
  String get mainPageFavorites => 'Favoritos';

  @override
  String get mainPageInbox => 'Caixa de entrada';

  @override
  String get mainPageLaboratory => 'Laboratório';

  @override
  String get mainPageOverview => 'Visão geral';

  @override
  String get mainPageProjects => 'Projetos';

  @override
  String mainPageSummary(String page) {
    return '$page · Visão geral';
  }

  @override
  String get mainPasteChanged =>
      'A entrada mudou durante a colagem. Reabra o editor.';

  @override
  String get mainPasteContent => 'Colar conteúdo';

  @override
  String get mainPause => 'Pausar';

  @override
  String get mainPersonalWorkspace => 'Espaço pessoal';

  @override
  String get mainPlay => 'Reproduzir';

  @override
  String get mainPluginSettings => 'Plugins e serviços';

  @override
  String get mainPluginSettingsSummary =>
      'Ferramentas integradas, extensões, rede, arquivos e proteção do conteúdo';

  @override
  String get mainPreviewEmpty => 'A prévia aparecerá aqui';

  @override
  String mainProgress(int done, int total) {
    return 'Pequenos passos · $done/$total';
  }

  @override
  String get mainProjectIntro =>
      'Avance com listas de tarefas. Cada pequeno passo aproxima você do objetivo.';

  @override
  String get mainQueryAgain => 'Nova consulta';

  @override
  String get mainQueryCapacity => 'Histórico de consultas cheio';

  @override
  String get mainQueryCapacityDetail =>
      'Seu conteúdo foi preservado. Esta versão ainda não permite limpar o histórico de consultas.';

  @override
  String get mainQueryLoading => 'Buscando ideias…';

  @override
  String get mainQueryRetry => 'Repetir consulta';

  @override
  String get mainQueryTerminated => 'Esta consulta terminou';

  @override
  String get mainQueryUnknown => 'Resultados ainda não confirmados';

  @override
  String get mainQuickHint => 'O que acabou de vir à mente?';

  @override
  String get mainReadOnlySettings =>
      'O conteúdo é somente leitura. Verifique o plugin do espaço de trabalho para restaurar a edição.';

  @override
  String get mainRecentThoughts => 'Ideias recentes';

  @override
  String get mainRecordedCount => 'Com observações';

  @override
  String get mainRestoreDefault => 'Restaurar padrão';

  @override
  String get mainRetry => 'Tentar novamente';

  @override
  String get mainRetrySave => 'Tentar salvar novamente';

  @override
  String get mainSage => 'Sálvia';

  @override
  String get mainSampleBody0 =>
      'Guarde pensamentos espontâneos aqui.\nSem pressa para terminar: deixe-os começar.';

  @override
  String get mainSampleBody1 =>
      'Uma página para palavras favoritas,\nmúsica e detalhes do dia a dia.';

  @override
  String get mainSampleBody2 =>
      'Experimente arte generativa. Deixe o código\ncriar formas inesperadas.';

  @override
  String get mainSampleBody3 =>
      'Um companheiro discreto para lembrar\ndas pequenas coisas que escapam.';

  @override
  String get mainSampleTitle0 => 'Um lar para as ideias';

  @override
  String get mainSampleTitle1 => 'Um jardim digital tranquilo';

  @override
  String get mainSampleTitle2 => 'Criar algo por diversão';

  @override
  String get mainSampleTitle3 => 'Meu pequeno assistente';

  @override
  String get mainSampleTodo0 => 'Organizar a primeira coleção';

  @override
  String get mainSampleTodo1 => 'Projetar a entrada do jardim';

  @override
  String get mainSampleTodo2 => 'Plantar uma nova ideia';

  @override
  String get mainSampleTodo3 => 'Esboçar um pequeno protótipo';

  @override
  String get mainSampleTodo4 => 'Projetar os lembretes';

  @override
  String get mainSaveConnectionUnknown =>
      'A conexão foi interrompida; o resultado do salvamento é desconhecido. Reabra a biblioteca e confira antes de tentar novamente.';

  @override
  String get mainSaveFailed =>
      'Não foi possível salvar. As alterações permanecem nesta sessão.';

  @override
  String get mainSaveIdea => 'Salvar ideia';

  @override
  String get mainSaveNotSubmitted =>
      'Não enviado. O rascunho e os anexos foram preservados; você pode editar e salvar novamente.';

  @override
  String get mainSaveReadOnly =>
      'As alterações não foram salvas. Ative o plugin do espaço de trabalho em Plugins e serviços e tente novamente.';

  @override
  String get mainSaveUnknown =>
      'Salvamento não confirmado. O rascunho e os anexos foram preservados. Repita este envio; ao fechar, o espaço de trabalho será atualizado para verificação.';

  @override
  String get mainSaving => 'Salvando…';

  @override
  String get mainSearchHint => 'Pesquisar ideias…';

  @override
  String get mainServiceRunSettings => 'Execução de serviços';

  @override
  String get mainServiceSettings => 'Serviços de API';

  @override
  String get mainSettings => 'Configurações';

  @override
  String get mainShowAppearance => 'Mostrar ajustes de aparência';

  @override
  String get mainSidebarMotto => 'Um pouco de ordem. Espaço para descobrir.';

  @override
  String get mainSlowProgress => 'Pequenos passos também levam adiante.';

  @override
  String get mainSolidCanvas => 'Liso';

  @override
  String get mainSolidDetail => 'Um fundo liso e tranquilo.';

  @override
  String get mainSolidOpacity => '100% · Opaco';

  @override
  String get mainSortFavorites => 'Favoritos primeiro';

  @override
  String get mainSortRecent => 'Adicionados recentemente';

  @override
  String get mainSortTitle => 'Por título';

  @override
  String get mainSquareCorners => '0 para cantos retos';

  @override
  String get mainStageActive => 'Em andamento';

  @override
  String get mainStageCompleted => 'Concluído';

  @override
  String get mainStageOrganized => 'Organizado';

  @override
  String get mainStagePlanned => 'Planejado';

  @override
  String get mainStageRecorded => 'Registrado';

  @override
  String mainStageTooltip(String title) {
    return 'Alterar etapa de $title';
  }

  @override
  String get mainStageUnsorted => 'A organizar';

  @override
  String get mainStageUnverified => 'A testar';

  @override
  String get mainStageVerifying => 'Em teste';

  @override
  String get mainStayCurious => 'MANTENHA A CURIOSIDADE. SEJA VOCÊ.';

  @override
  String mainSteps(int done, int total) {
    return '$done/$total etapas';
  }

  @override
  String get mainStorageUnavailable =>
      'Armazenamento local indisponível. As alterações duram apenas nesta sessão.';

  @override
  String get mainStorageUnreadable =>
      'Não foi possível ler o conteúdo salvo. Os dados originais estão preservados e não serão sobrescritos.';

  @override
  String get mainTenMinutesAgo => 'Há 10 minutos';

  @override
  String get mainTextureCanvas => 'Textura';

  @override
  String get mainTextureDetail =>
      'Uma fina textura de papel traz uma sensação tátil.';

  @override
  String get mainThemeCompass => 'Círculo de cores do tema';

  @override
  String get mainThemeGrayscale => 'Escala de cinza do tema';

  @override
  String get mainThemeTone => 'Cores do tema';

  @override
  String get mainThreeHoursAgo => 'Há 3 horas';

  @override
  String get mainTintOpacity => 'Opacidade da tonalidade';

  @override
  String get mainToProject => 'Mover para projetos';

  @override
  String get mainTodosPrompt => 'Próximos passos (um por linha, opcional)';

  @override
  String get mainTransparencyUnavailable =>
      'Não foi possível ativar a transparência do sistema. Você pode usar o fundo padrão.';

  @override
  String get mainTransparentCanvas => 'Transparente';

  @override
  String get mainTransparentDetail =>
      'Mostra o espaço atrás da janela; na web, o fundo da página.';

  @override
  String get mainUndo => 'Desfazer';

  @override
  String mainUnfavoriteTooltip(String title) {
    return 'Remover $title dos favoritos';
  }

  @override
  String get mainUnsortedCount => 'A organizar';

  @override
  String get mainUnverifiedCount => 'A testar';

  @override
  String get mainView => 'Ver';

  @override
  String get mainViewAll => 'Mostrar tudo';

  @override
  String get mainWarmSand => 'Areia quente';

  @override
  String get mainWhiteTheme => 'Claro';

  @override
  String get mainWindowRadius => 'Cantos da janela';

  @override
  String get mainWindowRadiusDetail =>
      'Ajuste independente da borda; ao maximizar, os cantos ficam retos';

  @override
  String get mainWindowsFrostOnly =>
      'O desfoque da área de trabalho está disponível apenas no Windows';

  @override
  String get mainWorkbench => 'Espaço de trabalho';

  @override
  String get mainWorkbenchPlugin => 'Plugin do espaço de trabalho';

  @override
  String get mainWriteHypothesis =>
      'Abra o registro e anote o que deseja testar.';

  @override
  String get mainYesterday => 'Ontem';

  @override
  String get pluginsApprovalUnknown =>
      'Não foi possível confirmar a ativação ou as mudanças de permissão';

  @override
  String get pluginsApproveEnable => 'Autorizar e ativar';

  @override
  String get pluginsApproveWorkbench => 'Permitir leitura e edição e ativar';

  @override
  String get pluginsAttachment => 'Ler anexos';

  @override
  String get pluginsBackingUp => 'Fazendo backup…';

  @override
  String get pluginsBackupLibrary => 'Fazer backup da biblioteca';

  @override
  String get pluginsBackupLibraryType => 'Backup da biblioteca';

  @override
  String get pluginsBackupProtection => 'Fazer backup da proteção';

  @override
  String get pluginsBackupUnknown =>
      'O backup ainda não foi confirmado. Preserve os arquivos criados e verifique o destino.';

  @override
  String pluginsBinaryPreview(String hex) {
    return 'Conteúdo binário: $hex';
  }

  @override
  String get pluginsBuiltin => 'Área integrada';

  @override
  String pluginsBuiltinCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count caracteres',
      one: '$count caractere',
    );
    return '$_temp0 · Apenas nesta sessão; não salvo como cartão';
  }

  @override
  String get pluginsBuiltinEmpty =>
      'Digite texto para visualizar em maiúsculas';

  @override
  String get pluginsBuiltinHeading => 'Ferramentas de texto';

  @override
  String get pluginsBuiltinInput => 'Digite um texto';

  @override
  String get pluginsCancel => 'Cancelar';

  @override
  String get pluginsChoosePackage => 'Escolher arquivo do plugin';

  @override
  String get pluginsChooseSmallFile => 'Escolher arquivo pequeno';

  @override
  String get pluginsCloseTextTool => 'Ocultar ferramenta de texto';

  @override
  String get pluginsCloseUnknown =>
      'Não foi possível confirmar o fechamento da visualização';

  @override
  String get pluginsCloseView => 'Fechar visualização';

  @override
  String get pluginsConnectionLost =>
      'Conexão interrompida. Reabra a visualização do plugin.';

  @override
  String get pluginsContentPermissions => 'Permissões de conteúdo';

  @override
  String get pluginsCreate => 'Criar conteúdo';

  @override
  String get pluginsCredentialCancel => 'Fechar formulário';

  @override
  String get pluginsCredentialCreateTitle => 'Nova credencial';

  @override
  String pluginsCredentialDays(int days) {
    String _temp0 = intl.Intl.pluralLogic(
      days,
      locale: localeName,
      other: '$days dias',
      one: '$days dia',
    );
    return '$_temp0';
  }

  @override
  String get pluginsCredentialDetails =>
      'Armazene credenciais de conexões API aprovadas com segurança. Salvar não autoriza servidores nem ativa plugins. Segredos salvos não podem ser visualizados.';

  @override
  String get pluginsCredentialDisable => 'Desativar';

  @override
  String get pluginsCredentialDisabled => 'Desativada';

  @override
  String get pluginsCredentialDisabledDone => 'Credencial desativada.';

  @override
  String get pluginsCredentialEmpty => 'Nenhuma credencial salva';

  @override
  String get pluginsCredentialExpired => 'Expirada';

  @override
  String pluginsCredentialExpires(String date) {
    return 'Expira em: $date';
  }

  @override
  String get pluginsCredentialHeader => 'Nome do cabeçalho';

  @override
  String get pluginsCredentialInvalid =>
      'Verifique o nome do cabeçalho e digite um novo segredo. O campo secreto foi limpo.';

  @override
  String get pluginsCredentialLifetime => 'Validade';

  @override
  String get pluginsCredentialLoadFailed =>
      'Não foi possível ler as credenciais de forma consistente. Atualize o estado para tentar novamente.';

  @override
  String get pluginsCredentialNew => 'Adicionar credencial';

  @override
  String get pluginsCredentialReading => 'Lendo credenciais…';

  @override
  String pluginsCredentialReference(String reference) {
    return 'Credencial $reference';
  }

  @override
  String get pluginsCredentialRefresh => 'Atualizar estado';

  @override
  String get pluginsCredentialReplace => 'Substituir segredo';

  @override
  String pluginsCredentialReplaceTitle(String reference) {
    return 'Substituir credencial $reference';
  }

  @override
  String get pluginsCredentialSave => 'Salvar credencial';

  @override
  String get pluginsCredentialSaved =>
      'Credencial salva. Conexões API ainda exigem aprovação separada.';

  @override
  String get pluginsCredentialSecret => 'Novo valor secreto';

  @override
  String get pluginsCredentialStored => 'Salva';

  @override
  String get pluginsCredentialTitle => 'Credenciais de API';

  @override
  String get pluginsCredentialUnknown =>
      'Não foi possível confirmar o resultado. O campo secreto foi limpo. Atualize o estado antes de outra alteração.';

  @override
  String pluginsDeclared(String permissions) {
    return 'Permissões declaradas: $permissions';
  }

  @override
  String get pluginsDependenciesNotice =>
      'As dependências devem ser configuradas no host. Esta página não as autoriza.';

  @override
  String get pluginsDisable => 'Desativar';

  @override
  String get pluginsDisableWorkbench => 'Desativar plugin da área';

  @override
  String get pluginsDisabled => 'Desativado';

  @override
  String get pluginsDisabledDetails =>
      'Desativado. Permita ler e editar conteúdo para usar o editor e as ferramentas.';

  @override
  String get pluginsEdit => 'Editar conteúdo';

  @override
  String get pluginsEmptyLibrary =>
      'Nenhum plugin de terceiros importado ainda.';

  @override
  String get pluginsEmptyResult => '(resultado vazio)';

  @override
  String get pluginsEnabled => 'Ativado';

  @override
  String get pluginsEnabledDetails =>
      'Ativado. Este plugin pode ler e editar conteúdo. Desativá-lo preserva seus dados.';

  @override
  String get pluginsEndpointAdvanced =>
      'Limites da política (bytes, salvo indicação)';

  @override
  String get pluginsEndpointCertificate => 'Escolher raiz DER';

  @override
  String get pluginsEndpointCertificateDetails =>
      'Raiz de confiança HTTPS opcional: um certificado DER binário (.der ou .cer), até 32 KiB. PEM e conjuntos de certificados não são aceitos. Remova a raiz antes de mudar para HTTP.';

  @override
  String get pluginsEndpointCertificateInvalid =>
      'Escolha um certificado DER binário válido (.der ou .cer) de até 32 KiB.';

  @override
  String pluginsEndpointCertificateSelected(int bytes) {
    return 'Raiz DER selecionada ($bytes bytes)';
  }

  @override
  String get pluginsEndpointConcurrency => 'Solicitações simultâneas (1–128)';

  @override
  String get pluginsEndpointCreateTitle => 'Nova aprovação de endpoint';

  @override
  String get pluginsEndpointCredential => 'Referência de credencial';

  @override
  String get pluginsEndpointCredentialLifetime =>
      'A credencial deve permanecer válida por toda a vigência do endpoint. Sua expiração não será estendida.';

  @override
  String get pluginsEndpointCredentialUnavailable =>
      'As credenciais exigem permissão de uso declarada e aprovada para o pacote e uma referência salva válida.';

  @override
  String get pluginsEndpointCredentialsFailed =>
      'Não foi possível ler as referências de credenciais. Atualize o estado antes de escolher.';

  @override
  String get pluginsEndpointDetails =>
      'Salve uma política de servidor para um pacote e hash específicos. Salvar não conecta à rede, não ativa plugins nem disponibiliza tarefas de rede imediatamente.';

  @override
  String get pluginsEndpointDigest => 'Hash do pacote';

  @override
  String get pluginsEndpointDisabledDone => 'Aprovação do endpoint desativada.';

  @override
  String get pluginsEndpointEmpty => 'Nenhuma aprovação de endpoint salva';

  @override
  String get pluginsEndpointFrameBytes => 'Limite de quadro (1–131072 bytes)';

  @override
  String get pluginsEndpointHeaderBytes =>
      'Máximo de bytes dos cabeçalhos (1–16384)';

  @override
  String get pluginsEndpointInvalid =>
      'Verifique pacote, origem, métodos, validade de 1 a 30 dias, permissão de credenciais, certificado e limites.';

  @override
  String get pluginsEndpointLifetime => 'Validade (1 a 30 dias)';

  @override
  String get pluginsEndpointLoadFailed =>
      'Não foi possível ler as aprovações de forma consistente. Atualize o estado para tentar novamente.';

  @override
  String get pluginsEndpointLocalHttp => 'HTTP local';

  @override
  String get pluginsEndpointLocalHttps => 'HTTPS local';

  @override
  String get pluginsEndpointMethods => 'Métodos de solicitação permitidos';

  @override
  String get pluginsEndpointNew => 'Adicionar endpoint';

  @override
  String get pluginsEndpointNoCredential => 'Sem credencial';

  @override
  String get pluginsEndpointOrigin =>
      'Somente origem, por exemplo https://api.example.com';

  @override
  String get pluginsEndpointPackage => 'Pacote';

  @override
  String get pluginsEndpointPackageUnavailable =>
      'O pacote está indisponível ou sem permissão HTTP aprovada. Aprovações existentes ainda podem ser desativadas.';

  @override
  String get pluginsEndpointProfile => 'Perfil de conexão';

  @override
  String get pluginsEndpointPublicHttps => 'HTTPS público';

  @override
  String get pluginsEndpointRemoveCertificate => 'Remover raiz de confiança';

  @override
  String get pluginsEndpointReplace => 'Substituir aprovação';

  @override
  String get pluginsEndpointReplaceTitle =>
      'Substituir aprovação usando o hash atual do pacote';

  @override
  String get pluginsEndpointRequestBytes =>
      'Máximo de bytes da solicitação (1–65536)';

  @override
  String get pluginsEndpointResponseBytes =>
      'Máximo de bytes da resposta (1–65536)';

  @override
  String get pluginsEndpointSave => 'Salvar aprovação do endpoint';

  @override
  String get pluginsEndpointSaved =>
      'Aprovação do endpoint salva. Nenhuma conexão de rede foi feita.';

  @override
  String get pluginsEndpointTimeout => 'Tempo limite (1–30000 milissegundos)';

  @override
  String get pluginsEndpointTitle => 'Aprovações de endpoints de API';

  @override
  String get pluginsEndpointUnknown =>
      'Não foi possível confirmar o resultado. Atualize o estado antes de outra alteração. A solicitação não será reenviada automaticamente.';

  @override
  String get pluginsEndpointWorking => 'Atualizando estado do endpoint…';

  @override
  String get pluginsExistingVersion =>
      'Esta versão já está instalada. Seu estado de ativação não mudou.';

  @override
  String pluginsFileLimit(int limit) {
    return 'O arquivo é muito grande. Escolha um de no máximo $limit bytes.';
  }

  @override
  String get pluginsHttpTaskAbandon => 'Encerrar observação desta tentativa';

  @override
  String get pluginsHttpTaskAbandonDetails =>
      'Só encerre a observação após um estado recente confirmar ausência de tarefa ativa e disponibilidade da biblioteca original. Isso não prova ausência de efeitos remotos. Identidade e incerteza ficam no histórico; uma nova solicitação exige outro envio explícito.';

  @override
  String get pluginsHttpTaskAbsent => 'Sem entrega de resultado';

  @override
  String get pluginsHttpTaskAccepted => 'Aceito';

  @override
  String get pluginsHttpTaskAcknowledge => 'Confirmar tarefa concluída';

  @override
  String get pluginsHttpTaskArchivedUnknown =>
      'Observação encerrada pelo usuário. Efeitos remotos anteriores continuam incertos; esta tentativa não foi repetida.';

  @override
  String get pluginsHttpTaskBase64 => 'Base64';

  @override
  String get pluginsHttpTaskBody => 'Corpo da solicitação';

  @override
  String get pluginsHttpTaskBodyFormat => 'Codificação do corpo da solicitação';

  @override
  String get pluginsHttpTaskBusy => 'Ocupado';

  @override
  String get pluginsHttpTaskCancel => 'Solicitar cancelamento';

  @override
  String get pluginsHttpTaskCancelled =>
      'Cancelamento detectado; efeitos remotos ainda podem ter ocorrido';

  @override
  String get pluginsHttpTaskCatalogUnavailable =>
      'O catálogo de plugins está indisponível. Atualize a biblioteca antes de um novo envio; os controles de tarefas existentes continuam disponíveis.';

  @override
  String get pluginsHttpTaskClosed => 'Fechado';

  @override
  String get pluginsHttpTaskCompleted => 'Concluído';

  @override
  String get pluginsHttpTaskConflict => 'Conflito';

  @override
  String get pluginsHttpTaskConsumed => 'Resultado consumido';

  @override
  String get pluginsHttpTaskControlUnknown =>
      'Não foi possível confirmar o resultado do controle. Atualize o estado antes da próxima ação.';

  @override
  String pluginsHttpTaskCounters(String bytes, String calls) {
    return 'Chamadas de E/S: $calls; bytes contabilizados: $bytes';
  }

  @override
  String get pluginsHttpTaskDeadline => 'Prazo excedido';

  @override
  String get pluginsHttpTaskDenied => 'Negado';

  @override
  String get pluginsHttpTaskDetails =>
      'Execute uma solicitação explícita com endpoint aprovado e plugin ativo com o encaminhador HTTP experimental. O estado permanece disponível enquanto a biblioteca está ocupada.';

  @override
  String get pluginsHttpTaskDisconnect => 'Falha na limpeza da conexão';

  @override
  String get pluginsHttpTaskEndpoint => 'Endpoint aprovado';

  @override
  String get pluginsHttpTaskEndpointsFailed =>
      'Não foi possível ler os endpoints consistentemente, ou a biblioteca está ocupada. Os controles continuam disponíveis. Atualize os endpoints quando a biblioteca retornar.';

  @override
  String get pluginsHttpTaskEvidenceUnavailable =>
      'Evidência do resultado indisponível';

  @override
  String pluginsHttpTaskExecution(int code, String fault) {
    return 'Execução do convidado: $fault; código de saída: $code';
  }

  @override
  String pluginsHttpTaskExit(
    String disconnect,
    String execution,
    String maintenance,
  ) {
    return 'Saída do worker — execução: $execution; desconexão: $disconnect; manutenção: $maintenance';
  }

  @override
  String get pluginsHttpTaskExplicit =>
      'Enviar faz uma solicitação real. Cada clique cria uma nova identidade. Envios e leituras incertos nunca são repetidos automaticamente. Cancelar não prova que a operação remota foi desfeita.';

  @override
  String get pluginsHttpTaskFailed => 'Falhou';

  @override
  String get pluginsHttpTaskHeaders =>
      'Cabeçalhos comuns, um Nome: valor por linha';

  @override
  String get pluginsHttpTaskHeadersHint =>
      'Cabeçalhos repetidos permanecem separados. Apenas o runtime fornece cabeçalhos de credenciais e conexão.';

  @override
  String get pluginsHttpTaskHistory => 'Observações anteriores (até 5)';

  @override
  String pluginsHttpTaskHttpResult(int code, String status) {
    return 'Resultado HTTP: $status; status remoto: $code';
  }

  @override
  String get pluginsHttpTaskInactive => 'Conexão inativa';

  @override
  String get pluginsHttpTaskInvalid =>
      'Verifique endpoint, método, destino relativo, cabeçalhos comuns, codificação do corpo e tempo limite conforme os limites aprovados.';

  @override
  String get pluginsHttpTaskInvalidOptions => 'Opções inválidas';

  @override
  String pluginsHttpTaskKey(String identity) {
    return 'Identidade da tarefa: $identity';
  }

  @override
  String get pluginsHttpTaskLimit => 'Cota ou limite atingido';

  @override
  String get pluginsHttpTaskLoadingEndpoints => 'Lendo endpoints aprovados…';

  @override
  String get pluginsHttpTaskLocal =>
      'Biblioteca disponível; nenhuma tarefa ativa';

  @override
  String get pluginsHttpTaskModule => 'Módulo convidado inválido';

  @override
  String get pluginsHttpTaskNew => 'Preparar nova solicitação';

  @override
  String get pluginsHttpTaskNoEndpoints =>
      'Nenhum endpoint corresponde a um plugin de encaminhamento HTTP ativo e aprovado.';

  @override
  String get pluginsHttpTaskNotFound => 'Não encontrado';

  @override
  String get pluginsHttpTaskOk => 'OK';

  @override
  String get pluginsHttpTaskOutcomeUnknown =>
      'Resultado remoto desconhecido; não presuma reversão nem reenvie';

  @override
  String get pluginsHttpTaskPackageChanged => 'Vinculação do pacote alterada';

  @override
  String get pluginsHttpTaskPending => 'Resultado pendente';

  @override
  String get pluginsHttpTaskPoll => 'Consultar tarefa';

  @override
  String get pluginsHttpTaskProtocol => 'Erro no protocolo da tarefa';

  @override
  String get pluginsHttpTaskRead => 'Ler resultado uma vez';

  @override
  String get pluginsHttpTaskReadBound => 'Limite de leitura excedido';

  @override
  String get pluginsHttpTaskReadPending =>
      'Nenhum resultado retornado. Verifique o estado antes de ler novamente de forma explícita.';

  @override
  String get pluginsHttpTaskReadUnknown =>
      'A leitura não foi confirmada e pode já ter consumido o resultado. Ele não será lido novamente. Estado e saída ainda podem ser consultados.';

  @override
  String get pluginsHttpTaskReady =>
      'Resultado pronto: leia explicitamente. Isso não significa que o worker encerrou.';

  @override
  String get pluginsHttpTaskReclaimed =>
      'Worker encerrado; biblioteca original devolvida';

  @override
  String get pluginsHttpTaskRecoveryRequired =>
      'Worker encerrado; limpeza ou manutenção exige reparo';

  @override
  String get pluginsHttpTaskRefresh => 'Atualizar estado da tarefa';

  @override
  String get pluginsHttpTaskRefreshEndpoints => 'Atualizar endpoints aprovados';

  @override
  String get pluginsHttpTaskRemoteError =>
      'O servidor remoto retornou 4xx/5xx. A troca HTTP foi concluída; isso é separado de erros de execução do convidado.';

  @override
  String get pluginsHttpTaskRepair => 'Reparar limpeza';

  @override
  String get pluginsHttpTaskResponseBase64 => 'Corpo da resposta: Base64 exato';

  @override
  String get pluginsHttpTaskResponseHeaders =>
      'Cabeçalhos de resposta (duplicados preservados; valores binários em Base64)';

  @override
  String get pluginsHttpTaskResponseText =>
      'Corpo da resposta: prévia em texto simples';

  @override
  String get pluginsHttpTaskResultUnavailable =>
      'Entrega do resultado indisponível';

  @override
  String get pluginsHttpTaskRevoked => 'Aprovação revogada';

  @override
  String get pluginsHttpTaskRunning => 'Em execução; worker detém a biblioteca';

  @override
  String get pluginsHttpTaskSpawn => 'Não foi possível iniciar o worker';

  @override
  String get pluginsHttpTaskStart => 'Enviar nova solicitação';

  @override
  String get pluginsHttpTaskStartUnknown =>
      'O resultado do envio é desconhecido. A identidade foi mantida. Consulte o estado da mesma tarefa; a solicitação não será reenviada.';

  @override
  String get pluginsHttpTaskStatusFailed =>
      'Não foi possível confirmar o estado. Atualize-o; nenhuma solicitação foi repetida.';

  @override
  String get pluginsHttpTaskStopping =>
      'Parando; aguardando saída real do worker';

  @override
  String pluginsHttpTaskSubmission(String identity) {
    return 'Identidade do envio: $identity';
  }

  @override
  String get pluginsHttpTaskTarget =>
      'Destino relativo, por exemplo /v1/items?limit=10';

  @override
  String get pluginsHttpTaskText => 'Texto UTF-8';

  @override
  String get pluginsHttpTaskTimeout =>
      'Tempo limite em ms (1–30000, dentro da aprovação)';

  @override
  String get pluginsHttpTaskTitle => 'Tarefas HTTP';

  @override
  String get pluginsHttpTaskTrap => 'Trap na execução do convidado';

  @override
  String get pluginsHttpTaskUnavailable =>
      'Biblioteca original indisponível; recuperação necessária';

  @override
  String get pluginsHttpTaskUnsupported => 'Operação não suportada';

  @override
  String get pluginsHttpTaskWorking =>
      'Aguardando resposta do controle da tarefa…';

  @override
  String get pluginsImport => 'Importar';

  @override
  String get pluginsImportDetails =>
      'Após importar, você decide se ativa o plugin. Desativar ou desinstalar preserva o conteúdo.';

  @override
  String pluginsImportPreview(String name) {
    return 'Prévia da importação: $name';
  }

  @override
  String get pluginsImportUnknown => 'Não foi possível confirmar a importação';

  @override
  String get pluginsImportedDisabled =>
      'Importado e desativado. Escolha as permissões a conceder.';

  @override
  String get pluginsInputFailed => 'Não foi possível ler o arquivo de entrada';

  @override
  String get pluginsInputTooLong =>
      'O limite de entrada foi atingido. Reduza o texto e tente novamente.';

  @override
  String get pluginsInspectFailed =>
      'Não foi possível carregar a prévia do plugin';

  @override
  String get pluginsInspectedOnly =>
      'O arquivo foi apenas inspecionado. Ative o plugin separadamente após importar.';

  @override
  String get pluginsInsufficientApproval =>
      'O plugin está ativado, mas precisa de permissão de conteúdo. A área permanece somente leitura. Desative-o para rever as permissões.';

  @override
  String pluginsIoApproved(String permissions) {
    return 'Aprovado: $permissions';
  }

  @override
  String get pluginsIoCredentialUse => 'Usar credenciais autorizadas';

  @override
  String pluginsIoDeclared(String permissions) {
    return 'Permissões de rede e arquivos solicitadas: $permissions';
  }

  @override
  String get pluginsIoFileCreate => 'Criar arquivos';

  @override
  String get pluginsIoFileDelete => 'Excluir arquivos';

  @override
  String get pluginsIoFileList => 'Explorar pastas autorizadas';

  @override
  String get pluginsIoFileRead => 'Ler arquivos autorizados';

  @override
  String get pluginsIoFileReplace => 'Substituir arquivos';

  @override
  String get pluginsIoHttpListen => 'Escutar conexões de rede';

  @override
  String get pluginsIoHttpPublish => 'Fornecer um serviço de API';

  @override
  String get pluginsIoHttpRequest => 'Chamar APIs de rede';

  @override
  String get pluginsIoNoneApproved =>
      'Nenhuma permissão de rede ou arquivos aprovada';

  @override
  String get pluginsIoRevoke =>
      'Revogar todas as permissões de rede e arquivos';

  @override
  String get pluginsIoSave => 'Salvar permissões de rede e arquivos';

  @override
  String get pluginsIoScopeNotice =>
      'Estas opções salvam apenas categorias de permissão. Endereços, acesso a arquivos e credenciais exigem aprovação separada; funções indisponíveis continuam indisponíveis. Reabra o formulário após alterações.';

  @override
  String get pluginsIoTitle => 'Permissões de rede e arquivos';

  @override
  String get pluginsIoWebSocketConnect => 'Conectar a serviços WebSocket';

  @override
  String get pluginsListUnknown =>
      'Não foi possível confirmar a lista de plugins';

  @override
  String get pluginsManageAbove =>
      'Gerencie este plugin pelos controles da área de trabalho acima.';

  @override
  String get pluginsManagementUnavailable =>
      'O gerenciamento de plugins está indisponível. O conteúdo existente continua legível.';

  @override
  String get pluginsNoPermissions => 'Nenhuma permissão de conteúdo declarada.';

  @override
  String get pluginsOpenTextTool => 'Abrir ferramenta de texto';

  @override
  String get pluginsOpenView => 'Abrir visualização';

  @override
  String get pluginsOpeningView => 'Abrindo visualização do plugin…';

  @override
  String get pluginsOperation => 'Consultar resultados de operações';

  @override
  String pluginsOtherCapability(String name) {
    return 'Outra permissão declarada: $name';
  }

  @override
  String get pluginsPackageFile => 'Plugin do Morrow';

  @override
  String get pluginsPreviewOnly =>
      'Somente prévia. Os resultados não são salvos automaticamente no conteúdo existente.';

  @override
  String get pluginsPreviewTruncated =>
      '…exibindo apenas os primeiros 4.096 caracteres';

  @override
  String get pluginsProtection => 'Proteção do conteúdo';

  @override
  String get pluginsProtectionDetails =>
      'Salve o arquivo de proteção original para recuperar com esta conta do sistema. Ele não contém cartões nem anexos.';

  @override
  String get pluginsProtectionFileType => 'Arquivo de proteção da biblioteca';

  @override
  String get pluginsProtectionSaved =>
      'Arquivo de proteção salvo. Se a inicialização falhar, selecione-o para recuperar.';

  @override
  String get pluginsRead => 'Ler conteúdo';

  @override
  String get pluginsReadingState => 'Carregando estado do plugin…';

  @override
  String pluginsRefreshFailed(String reason) {
    return '$reason. Não foi possível atualizar a lista. Selecione “Atualizar lista” para tentar ler novamente.';
  }

  @override
  String get pluginsRefreshList => 'Atualizar lista';

  @override
  String get pluginsRefreshState => 'Atualizar estado';

  @override
  String get pluginsRename => 'Renomear';

  @override
  String pluginsResultBytes(int count, String preview) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count bytes',
      one: '$count byte',
    );
    return '$_temp0\n$preview';
  }

  @override
  String get pluginsSavePermissions => 'Salvar permissões';

  @override
  String pluginsSelectedFile(String name) {
    return 'Arquivo selecionado: $name';
  }

  @override
  String get pluginsServiceAcknowledgeUncertain =>
      'Revisei os registros atualizados';

  @override
  String get pluginsServiceAddScope => 'Adicionar escopo de conteúdo';

  @override
  String get pluginsServiceAttachmentId => 'Identidade exata do anexo';

  @override
  String get pluginsServiceAuthenticationUnavailable =>
      'A autenticação está ausente, desativada, expirada ou pertence a outra identidade. Seus escopos provisórios são mantidos; selecione uma substituição válida ou remova-a explicitamente.';

  @override
  String get pluginsServiceAuthorities =>
      'Registros de autenticação e publicação';

  @override
  String get pluginsServiceCardId => 'Identidade exata do cartão';

  @override
  String get pluginsServiceCatalogChanged =>
      'O catálogo mudou ou está indisponível. Seu rascunho foi preservado. Atualize a seleção explicitamente antes de salvar.';

  @override
  String get pluginsServiceClearToken => 'Limpar token';

  @override
  String get pluginsServiceCloseEditor => 'Fechar editor';

  @override
  String get pluginsServiceConfigDigest => 'Hash da configuração';

  @override
  String get pluginsServiceConfiguration => 'Configuração salva';

  @override
  String get pluginsServiceConfigurations => 'Configurações salvas';

  @override
  String get pluginsServiceCopyClear => 'Copiar e limpar token';

  @override
  String get pluginsServiceCreated => 'Criação (UTC)';

  @override
  String get pluginsServiceDays => 'Validade solicitada (1 a 30 dias)';

  @override
  String get pluginsServiceDigestFixed =>
      'A edição mantém o hash original. Selecione um pacote correspondente; isso não o ativa.';

  @override
  String get pluginsServiceDisable => 'Desativar';

  @override
  String get pluginsServiceDisabled => 'Desativado';

  @override
  String get pluginsServiceEditConfig => 'Editar configuração';

  @override
  String get pluginsServiceEditPublication => 'Editar publicação';

  @override
  String get pluginsServiceExpired => 'Expirado ou ainda não válido';

  @override
  String get pluginsServiceExpires => 'Expiração real (UTC)';

  @override
  String get pluginsServiceHandler => 'Manipulador de serviço declarado';

  @override
  String get pluginsServiceIdentity => 'Identidade do serviço';

  @override
  String get pluginsServiceInvalid =>
      'Verifique campos, aprovações selecionadas e pacote atual antes de salvar.';

  @override
  String get pluginsServiceIssue => 'Emitir token';

  @override
  String get pluginsServiceIssuedToken => 'Token Bearer de exibição única';

  @override
  String get pluginsServiceListenAddress =>
      'Endereço numérico e porta de escuta';

  @override
  String get pluginsServiceLoadFailed =>
      'Não foi possível atualizar os registros. Atualize novamente antes de alterar.';

  @override
  String get pluginsServiceManagementOnly =>
      'Gerencie configurações e aprovações salvas. Salvar não inicia um listener, executa um pacote nem ativa um serviço.';

  @override
  String get pluginsServiceMethod => 'Método HTTP';

  @override
  String get pluginsServiceNewAuthentication => 'Nova autenticação';

  @override
  String get pluginsServiceNewConfig => 'Nova configuração';

  @override
  String get pluginsServiceNo => 'Não';

  @override
  String get pluginsServiceNoAuthentication =>
      'Primeiro crie um registro de autenticação válido.';

  @override
  String get pluginsServiceNoAuthorities =>
      'Nenhum registro de autenticação ou publicação.';

  @override
  String get pluginsServiceNoConfigurations =>
      'Nenhuma configuração de serviço.';

  @override
  String get pluginsServicePackage => 'Pacote declarado e aprovado';

  @override
  String get pluginsServicePackageDigest => 'Hash do pacote';

  @override
  String get pluginsServicePackageUnavailable =>
      'O pacote correspondente ou suas aprovações de escuta/publicação estão indisponíveis. Registros históricos continuam legíveis e podem ser desativados.';

  @override
  String get pluginsServicePath => 'Caminho exato da solicitação';

  @override
  String get pluginsServicePolicyChanged =>
      'O registro original mudou ou não é mais utilizável. Atualize a seleção ou reabra o editor a partir do registro atual. Seu rascunho foi preservado.';

  @override
  String get pluginsServicePrincipalId => 'Identificador da identidade';

  @override
  String get pluginsServicePrincipals =>
      'Identidades autorizadas e escopos de conteúdo';

  @override
  String get pluginsServicePublicationEditor => 'Aprovação de publicação';

  @override
  String get pluginsServicePublicationHelp =>
      'A aprovação se vincula a esta configuração, revisão e referência exatas. A expiração é limitada por todas as autenticações selecionadas e pode ser menor que a solicitada. Salvar não inicia a escuta.';

  @override
  String get pluginsServicePublicationMismatch =>
      'A publicação não corresponde mais à configuração atual. Revise e salve explicitamente uma aprovação substituta.';

  @override
  String get pluginsServiceQueryPath =>
      'Caminho separado de consulta do resultado (opcional)';

  @override
  String get pluginsServiceReference => 'Referência da aprovação';

  @override
  String get pluginsServiceRefresh => 'Atualizar registros';

  @override
  String get pluginsServiceRefreshSelection => 'Atualizar esta seleção';

  @override
  String get pluginsServiceRemovePrincipal => 'Remover identidade';

  @override
  String get pluginsServiceRemoveScope => 'Remover escopo';

  @override
  String get pluginsServiceRetention =>
      'Retenção do histórico (ms, até 30 dias)';

  @override
  String get pluginsServiceRevision => 'Revisão';

  @override
  String get pluginsServiceRotate => 'Rotacionar token';

  @override
  String get pluginsServiceRotateAuthentication => 'Rotacionar autenticação';

  @override
  String get pluginsServiceRunAbandon => 'Manter registro e encerrar tentativa';

  @override
  String get pluginsServiceRunAdvanced => 'Limites de solicitações e worker';

  @override
  String get pluginsServiceRunAttempt => 'Tentativa de início não resolvida';

  @override
  String get pluginsServiceRunBoundsHint =>
      'Os limites também devem respeitar a declaração do plugin e as aprovações salvas. Trabalho reservado consome o limite acumulado mesmo quando cancelado. A expiração encerra a execução; não há renovação automática.';

  @override
  String get pluginsServiceRunBytes =>
      'Limite de bytes da execução (até 67.108.864)';

  @override
  String get pluginsServiceRunCalls => 'Chamadas por tarefa (até 1.024)';

  @override
  String get pluginsServiceRunCancelled => 'Cancelado';

  @override
  String get pluginsServiceRunClosed => 'Fechado';

  @override
  String get pluginsServiceRunConcurrent => 'Tarefas simultâneas (até 128)';

  @override
  String get pluginsServiceRunControlUnknown =>
      'O resultado do controle é desconhecido. Atualize o estado do serviço original antes de outra operação.';

  @override
  String get pluginsServiceRunDenied => 'Negado';

  @override
  String get pluginsServiceRunExited => 'Serviço encerrado';

  @override
  String get pluginsServiceRunHeaderBytes =>
      'Tamanho máximo dos cabeçalhos (bytes, até 65.536)';

  @override
  String get pluginsServiceRunHint =>
      'Escolha uma publicação aprovada e limites finitos, então inicie o serviço explicitamente. Após parar, aguarde o retorno do proprietário original da área antes de confirmar o resultado.';

  @override
  String pluginsServiceRunHostFailure(String detail) {
    return 'Diagnóstico do host: $detail';
  }

  @override
  String get pluginsServiceRunHttpPanel =>
      'Um serviço API detém esta tarefa. Use o painel acima para pará-lo ou confirmar sua saída. Seu rascunho HTTP foi preservado.';

  @override
  String get pluginsServiceRunIdentityChanged =>
      'Outra tarefa detém o conteúdo. Este painel não a controlará usando a identidade anterior do serviço.';

  @override
  String get pluginsServiceRunInvalid =>
      'Verifique o serviço e os limites numéricos. Nenhuma nova execução foi enviada.';

  @override
  String get pluginsServiceRunInvalidOutcome => 'Configuração inválida';

  @override
  String get pluginsServiceRunJobBytes => 'Bytes por tarefa (até 16.777.216)';

  @override
  String get pluginsServiceRunJobs =>
      'Reservas totais de tarefas (até 1.000.000)';

  @override
  String get pluginsServiceRunLastObservation =>
      'Exibindo a última observação; estado atual não verificado.';

  @override
  String get pluginsServiceRunLifetime => 'Duração (ms, até 3.600.000)';

  @override
  String get pluginsServiceRunLimit => 'Limite atingido';

  @override
  String get pluginsServiceRunLocal => 'Conteúdo disponível localmente';

  @override
  String pluginsServiceRunNetwork(
    String bind,
    String listener,
    String supervision,
  ) {
    return 'Vinculação: $bind; listener: $listener; supervisão: $supervision';
  }

  @override
  String get pluginsServiceRunNextSettings =>
      'Configurações da próxima execução explícita';

  @override
  String get pluginsServiceRunNoSelection =>
      'Nenhuma publicação aprovada disponível. Verifique plugin, configuração e autenticação.';

  @override
  String get pluginsServiceRunOutboundAttempt =>
      'Endpoints vinculados a esta tentativa de início';

  @override
  String get pluginsServiceRunOutboundClear => 'Limpar seleção de endpoints';

  @override
  String get pluginsServiceRunOutboundFailed =>
      'Não foi possível verificar a lista de endpoints. Atualize antes de usar os selecionados.';

  @override
  String get pluginsServiceRunOutboundHint =>
      'APIs de saída (opcional, até 8). Só aparecem endpoints aprovados para este pacote. Sem seleção, chamadas de saída são bloqueadas.';

  @override
  String get pluginsServiceRunOutboundStale =>
      'Um endpoint selecionado mudou ou não está disponível. Selecione sua versão atual explicitamente ou limpe a seleção.';

  @override
  String get pluginsServiceRunOwned => 'Conteúdo gerenciado pelo serviço ativo';

  @override
  String get pluginsServiceRunPending => 'Pendente';

  @override
  String get pluginsServiceRunReclaimed =>
      'Propriedade do conteúdo recuperada; confirmação necessária';

  @override
  String get pluginsServiceRunReclaiming =>
      'Aguardando devolução da propriedade do conteúdo';

  @override
  String get pluginsServiceRunRecovery => 'Limpeza exige reparo';

  @override
  String get pluginsServiceRunRequestBytes =>
      'Tamanho máximo da solicitação (bytes)';

  @override
  String get pluginsServiceRunResponseBytes =>
      'Tamanho máximo da resposta (bytes)';

  @override
  String get pluginsServiceRunRunning => 'Serviço em execução';

  @override
  String get pluginsServiceRunSelection => 'Publicação de serviço aprovada';

  @override
  String get pluginsServiceRunStale =>
      'O pacote, configuração ou aprovação mudou. Atualize os registros e selecione novamente antes de iniciar.';

  @override
  String get pluginsServiceRunStart => 'Iniciar serviço limitado';

  @override
  String get pluginsServiceRunStartRejected =>
      'A resposta de início indicou um erro. A tarefa atual foi verificada; revise a causa e o estado da limpeza antes de continuar.';

  @override
  String get pluginsServiceRunStartUnknown =>
      'O resultado do início é desconhecido. A identidade da tentativa foi mantida; atualize para localizá-la. Não será iniciado novamente de forma automática.';

  @override
  String get pluginsServiceRunStarting => 'Iniciando serviço';

  @override
  String get pluginsServiceRunStatusFailed =>
      'Não foi possível verificar o estado do serviço. Atualize antes de agir.';

  @override
  String get pluginsServiceRunStop => 'Parar serviço';

  @override
  String get pluginsServiceRunStopping =>
      'Parando; aguardando saída do listener e worker';

  @override
  String get pluginsServiceRunSucceeded => 'Sucesso';

  @override
  String get pluginsServiceRunTask => 'Identidade da tarefa atual';

  @override
  String get pluginsServiceRunTimeout =>
      'Tempo limite por tarefa (ms, até 30.000)';

  @override
  String get pluginsServiceRunTimeoutOutcome => 'Tempo esgotado';

  @override
  String get pluginsServiceRunTitle => 'Executar serviço de API';

  @override
  String get pluginsServiceRunTotalBytes =>
      'Limite de bytes do worker (até 67.108.864)';

  @override
  String get pluginsServiceRunTransport => 'Falha de transporte';

  @override
  String get pluginsServiceRunUnavailable =>
      'Armazenamento de conteúdo indisponível';

  @override
  String get pluginsServiceSaveConfig => 'Salvar configuração';

  @override
  String get pluginsServiceSavePublication => 'Salvar aprovação de publicação';

  @override
  String get pluginsServiceSaved =>
      'Salvo. Confira abaixo a revisão retornada e a expiração real.';

  @override
  String get pluginsServiceScopeAttachment => 'Ler anexo';

  @override
  String get pluginsServiceScopeCreate => 'Criar conteúdo';

  @override
  String get pluginsServiceScopeEdit => 'Editar conteúdo';

  @override
  String get pluginsServiceScopeKind => 'Operação de conteúdo permitida';

  @override
  String get pluginsServiceScopeQuery => 'Consultar operação';

  @override
  String get pluginsServiceScopeRead => 'Ler conteúdo';

  @override
  String get pluginsServiceScopeRename => 'Renomear cartão';

  @override
  String get pluginsServiceScopeSummary => 'Ler resumo';

  @override
  String get pluginsServiceScopesHelp =>
      'Selecione a autenticação explicitamente. Adicione cada operação permitida e a identidade exata do objeto abaixo. Remover um escopo ou identidade exige seu próprio botão; escopos existentes são preservados na edição.';

  @override
  String get pluginsServiceTitle => 'Configuração do serviço';

  @override
  String get pluginsServiceTls => 'Exigir TLS';

  @override
  String get pluginsServiceTlsAttempt =>
      'Hash do certificado PEM vinculado à tentativa';

  @override
  String get pluginsServiceTlsCertificate => 'Escolher cadeia de certificados';

  @override
  String get pluginsServiceTlsChecked =>
      'Certificado e chave verificados. O SHA-256 do PEM está abaixo. Clientes ainda devem verificar nome do host, validade e cadeia de confiança.';

  @override
  String get pluginsServiceTlsChecking => 'Processando seleção do certificado…';

  @override
  String get pluginsServiceTlsFailed =>
      'Falha na verificação. Confira arquivos PEM, correspondência da chave e caminhos locais antes de tentar novamente.';

  @override
  String get pluginsServiceTlsHelp =>
      'Endereços fora de loopback exigem TLS. Apenas o requisito é salvo; nenhum listener ou identidade TLS é criado aqui.';

  @override
  String get pluginsServiceTlsHint =>
      'Selecione uma cadeia PEM e chave privada, então verifique-as. Os arquivos são verificados novamente no início; o certificado ativo não é rotacionado automaticamente.';

  @override
  String get pluginsServiceTlsInspect => 'Verificar certificado';

  @override
  String get pluginsServiceTlsOutsideValidity =>
      'A cadeia ainda não é válida ou expirou. Verifique ou substitua o certificado e inspecione novamente antes de iniciar.';

  @override
  String get pluginsServiceTlsPrivateKey => 'Escolher chave privada';

  @override
  String get pluginsServiceTlsRecheck =>
      'Verifique novamente o certificado antes de iniciar. Uma mudança no relógio não restaura a seleção anterior.';

  @override
  String get pluginsServiceTlsUnavailable =>
      'Este backend não suporta seleção de certificado TLS local.';

  @override
  String pluginsServiceTlsValidity(String end, String start) {
    return 'Validade comum da cadeia (UTC): de $start a $end. O serviço para após expirar.';
  }

  @override
  String get pluginsServiceTokenDiscarded =>
      'O token de exibição única foi limpo ao fechar o painel. Emita outro explicitamente se necessário.';

  @override
  String get pluginsServiceTokenHelp =>
      'Este token só é exibido agora. Copie-o explicitamente se necessário. Limpar ou fechar o painel o remove da sessão; ele não pode ser recuperado da lista. A rotação substitui o token anterior.';

  @override
  String get pluginsServiceUncertainHelp =>
      'Primeiro atualize e confira os registros originais. Confirmar este aviso só permite outra ação explícita; não prova que a alteração anterior falhou nem a repete.';

  @override
  String get pluginsServiceUnsupported => 'Valor histórico não suportado';

  @override
  String get pluginsServiceWorking => 'Processando…';

  @override
  String get pluginsServiceWriteUnknown =>
      'O resultado da última alteração é desconhecido. Ela não foi reenviada.';

  @override
  String get pluginsServiceYes => 'Sim';

  @override
  String get pluginsSettingsUnknown =>
      'O ajuste ainda não foi confirmado. Atualize o estado antes de escolher novamente.';

  @override
  String get pluginsSnapshotDetails =>
      'O backup inclui cartões, anexos e registros de auditoria. Recursos externos permanecem referências. A recuperação exige a conta original do sistema.';

  @override
  String get pluginsSnapshotSaved =>
      'Biblioteca salva, incluindo anexos e arquivo de proteção original.';

  @override
  String get pluginsStateUnavailable =>
      'Não foi possível carregar o estado do plugin. Tente novamente.';

  @override
  String get pluginsSummary => 'Ler resumos';

  @override
  String get pluginsTextInput => 'Texto de entrada';

  @override
  String get pluginsThirdParty => 'Plugins de terceiros';

  @override
  String get pluginsTlsIdentitiesDisable => 'Desativar identidade';

  @override
  String get pluginsTlsIdentitiesEmpty =>
      'Nenhuma identidade salva nesta biblioteca.';

  @override
  String get pluginsTlsIdentitiesFileMode =>
      'Próximo início: arquivos locais verificados.';

  @override
  String get pluginsTlsIdentitiesHint =>
      'Selecione uma identidade explicitamente. Substituí-la ou desativá-la para os serviços que a utilizam; um novo início é sempre explícito.';

  @override
  String get pluginsTlsIdentitiesImport =>
      'Preparar certificados para importar ou substituir';

  @override
  String get pluginsTlsIdentitiesReplace =>
      'Substituir por arquivos verificados';

  @override
  String get pluginsTlsIdentitiesSave => 'Salvar como nova identidade';

  @override
  String get pluginsTlsIdentitiesSaved =>
      'Salvo. Revise a identidade e revisão abaixo, então selecione para um novo início.';

  @override
  String get pluginsTlsIdentitiesSavedMode =>
      'Próximo início: identidade salva. O host verifica a validade do certificado ao iniciar.';

  @override
  String get pluginsTlsIdentitiesSelect => 'Usar no próximo início';

  @override
  String get pluginsTlsIdentitiesStale =>
      'A identidade mudou, foi desativada ou não foi atualizada. Selecione novamente uma identidade atual.';

  @override
  String get pluginsTlsIdentitiesTitle => 'Identidades TLS salvas';

  @override
  String get pluginsTlsIdentitiesUnknownHint =>
      'Atualize e confira os registros antes de confirmar. A ausência de recibo não significa falha; não recrie sem verificar.';

  @override
  String get pluginsTlsIdentitiesUseFile =>
      'Usar arquivos verificados no próximo início';

  @override
  String get pluginsTransform => 'Transformar';

  @override
  String get pluginsTransformUnknown =>
      'Não foi possível confirmar a transformação';

  @override
  String get pluginsUiExecution =>
      'A execução do plugin não terminou. Reabra a visualização e tente novamente.';

  @override
  String get pluginsUiRejected =>
      'A ação do plugin não foi aceita. Verifique a entrada e as permissões atuais.';

  @override
  String get pluginsUiUnavailable =>
      'O plugin está indisponível. Verifique o estado e reabra a visualização.';

  @override
  String get pluginsUnavailableView => 'Visualização do plugin indisponível';

  @override
  String pluginsUnconfirmed(String reason) {
    return '$reason. A operação não está confirmada. Verifique o estado atualizado antes de escolher novamente.';
  }

  @override
  String get pluginsUninstallKeepContent => 'Desinstalar (manter conteúdo)';

  @override
  String get pluginsUninstallUnknown =>
      'Não foi possível confirmar a desinstalação';

  @override
  String get pluginsUninstalled =>
      'Desinstalado. Seu conteúdo existente foi preservado.';

  @override
  String get pluginsUpdatingView => 'Atualizando prévia…';

  @override
  String get pluginsUseText => 'Usar texto';

  @override
  String get pluginsUseTransform => 'Usar transformação';

  @override
  String get pluginsViewFailed =>
      'Não foi possível abrir a visualização do plugin';

  @override
  String get pluginsWorkbench => 'Plugin da área de trabalho';

  @override
  String get pluginsWorkbenchReadOnly =>
      'O plugin está autorizado, mas a área é somente leitura. Resolva o problema da biblioteca ou do plugin e atualize o estado.';

  @override
  String get recoveryAllFiles => 'Todos os arquivos';

  @override
  String get recoveryBackupExists =>
      'Já existe um arquivo no destino. Escolha outro nome.';

  @override
  String get recoveryBackupFile => 'Backup da biblioteca';

  @override
  String get recoveryBackupUnknown =>
      'Verifique o resultado do backup. Preserve o arquivo atual e confira o local.';

  @override
  String get recoveryBindingMissing =>
      'Esta biblioteca não tem arquivo de proteção vinculado. Não é possível associar o arquivo escolhido.';

  @override
  String get recoveryBusy =>
      'Outro processo usa esta biblioteca. Feche a outra janela e tente novamente.';

  @override
  String get recoveryChooseKey => 'Escolher arquivo de recuperação';

  @override
  String get recoveryCloseFirst =>
      'O espaço de trabalho ainda está aberto. Feche antes de trocar de biblioteca.';

  @override
  String get recoveryFailed =>
      'A recuperação não terminou. Preserve os arquivos originais e tente novamente.';

  @override
  String get recoveryIdentityBusy =>
      'Outra cópia desta biblioteca está aberta. Feche-a antes de abrir esta.';

  @override
  String get recoveryIdentityMismatch =>
      'A identidade registrada não corresponde. Preserve os dados originais e restaure o backup correto.';

  @override
  String get recoveryKeyFile => 'Arquivo de proteção da biblioteca';

  @override
  String get recoveryKeyGuide =>
      'Se o arquivo de proteção estiver ausente ou danificado, escolha seu backup. Ele deve pertencer a esta biblioteca e exige a conta original do sistema.';

  @override
  String get recoveryKeyMismatch =>
      'A chave não corresponde ou não pode ser descriptografada. Use o arquivo e a conta de sistema originais.';

  @override
  String get recoveryKeyUnknown =>
      'Verifique o resultado da recuperação. Tente reabrir; uma cópia do arquivo de proteção anterior foi mantida, se existia.';

  @override
  String get recoveryLibraryInvalid =>
      'Não foi possível verificar ou abrir a biblioteca. Preserve a biblioteca e a chave originais e tente novamente.';

  @override
  String get recoveryMaintenance =>
      'A biblioteca precisa de atenção. Preserve os originais e examine o diagnóstico.';

  @override
  String get recoveryMissingKey =>
      'A chave de proteção está ausente. Restaure o .audit-key original e tente novamente.';

  @override
  String get recoveryMissingLibrary =>
      'A chave existe, mas a biblioteca está ausente ou vazia. Restaure a biblioteca original.';

  @override
  String get recoveryOpenFailed =>
      'Não foi possível abrir o espaço de trabalho. Verifique os plugins e a pasta de dados e tente novamente.';

  @override
  String get recoveryPluginUnavailable =>
      'Plugin do espaço de trabalho indisponível. O conteúdo existente pode ser visualizado e exportado.';

  @override
  String get recoveryRegistryInvalid =>
      'O registro da biblioteca ativa está danificado ou não é compatível. A abertura foi interrompida para preservar os dados.';

  @override
  String get recoveryRegistryUnreadable =>
      'Não é possível ler a biblioteca ativa ou seu registro. Verifique o local original; nenhuma biblioteca substituta será criada automaticamente.';

  @override
  String get recoveryRetry => 'Tentar novamente';

  @override
  String get recoverySnapshot => 'Restaurar backup da biblioteca';

  @override
  String get recoverySnapshotGuide =>
      'Restaure o backup da biblioteca em uma nova pasta e alterne para ela. A pasta original é preservada. O conteúdo volta ao momento do backup e exige a conta original do sistema.';

  @override
  String get recoverySnapshotInvalid =>
      'Formato ou integridade do backup inválidos. Preserve o arquivo original.';

  @override
  String get recoverySnapshotUnknown =>
      'Verifique o resultado na pasta de destino; a biblioteca original não foi substituída.';

  @override
  String recoverySwitchUnconfirmed(String path) {
    return 'Backup restaurado em $path, mas a troca não foi confirmada. Preserve a pasta e reabra o espaço para verificar.';
  }

  @override
  String get recoverySwitchUnknown =>
      'Troca de biblioteca não confirmada. Reabra o espaço de trabalho para verificar.';

  @override
  String get recoveryTargetExists =>
      'O destino já existe. Escolha uma nova pasta que ainda não exista.';

  @override
  String get recoveryTitle => 'Reabrir espaço de trabalho';

  @override
  String get visualApplyColor => 'Aplicar cor';

  @override
  String get visualApplyComponent => 'Aplicar a este componente';

  @override
  String get visualApplyTexture => 'Aplicar mídia';

  @override
  String visualAttachmentDetails(String action, String extension, String size) {
    return '$extension · $size · $action';
  }

  @override
  String get visualAttachmentFailure =>
      'A operação de arquivo falhou. Verifique o arquivo e o espaço disponível.';

  @override
  String get visualAttachmentPreview => 'Prévia do anexo local';

  @override
  String get visualAttachmentReadFailure =>
      'Não foi possível ler o anexo. Importe novamente.';

  @override
  String get visualAudio => 'Áudio';

  @override
  String get visualAudioStateFailure =>
      'Não foi possível confirmar o estado do áudio. Tente novamente.';

  @override
  String get visualAutoLyrics =>
      'Buscar letras ausentes online automaticamente';

  @override
  String get visualCancel => 'Cancelar';

  @override
  String get visualChangeCover => 'Alterar capa';

  @override
  String get visualChooseAudio =>
      'Escolha um arquivo de áudio ou um LRC com o mesmo nome.';

  @override
  String get visualChooseLyrics => 'Escolha um arquivo de letras LRC ou TXT.';

  @override
  String get visualClickPreview => 'Selecione para pré-visualizar';

  @override
  String get visualClose => 'Fechar';

  @override
  String get visualCloseDialog => 'Fechar diálogo';

  @override
  String get visualCloseWindow => 'Fechar janela';

  @override
  String get visualCollapsePlaylist => 'Recolher lista';

  @override
  String get visualColorGuide =>
      'Arraste o círculo para escolher matiz e saturação, depois ajuste o brilho. Você também pode inserir uma cor.';

  @override
  String get visualColorTitle => 'Dê cor ao seu espaço';

  @override
  String visualComponentCompass(String title) {
    return '$title · Círculo de cores';
  }

  @override
  String get visualComponents => 'Componentes e cartões';

  @override
  String get visualComponentsGuide =>
      'Cada item segue o tema por padrão. Personalize um cartão sem alterar os outros.';

  @override
  String get visualCornerTips1 =>
      'Nem toda ideia precisa ser útil.\nAlgumas apenas tornam o dia mais interessante.';

  @override
  String get visualCornerTips2 =>
      'Anote e deixe crescer.\nUma ideia não precisa nascer completa.';

  @override
  String get visualCornerTips3 =>
      'Deixe um pouco de espaço para você.\nA curiosidade precisa respirar.';

  @override
  String get visualCornerTips4 =>
      'Tente algo novo hoje.\nUm pequeno desvio pode surpreender.';

  @override
  String get visualCornerTips5 =>
      'Devaneios podem levar a algum lugar.\nDeixe seus pensamentos passearem.';

  @override
  String get visualCornerTips6 =>
      'Reserve tempo para o que ama.\nNão é preciso provar seu valor.';

  @override
  String get visualCornerTips7 =>
      'O progresso pode ser pequeno.\nQuerer começar já importa.';

  @override
  String get visualCornerTips8 =>
      'Olhe pela janela de vez em quando.\nA vida também inspira.';

  @override
  String get visualCover => 'Capa';

  @override
  String get visualCustomCompass => 'Círculo de cores · Personalizado';

  @override
  String get visualCustomMaterialGuide =>
      'Desative para seguir o tema e manter os ajustes personalizados deste item.';

  @override
  String get visualDefaultOpen => 'Abrir com aplicativo padrão';

  @override
  String get visualDownloadOpen => 'Baixar para abrir';

  @override
  String get visualEmbeddedLyrics => 'Incorporadas ao áudio';

  @override
  String get visualExpandPlaylist => 'Expandir lista';

  @override
  String get visualFile => 'Arquivo';

  @override
  String get visualFileOpenFailure =>
      'Não foi possível abrir o arquivo. Instale um aplicativo compatível ou salve o anexo e abra nele.';

  @override
  String get visualFileRetry =>
      'A operação de arquivo falhou. Tente novamente.';

  @override
  String get visualFindLyrics => 'Encontrar letras';

  @override
  String get visualFindLyricsGuide =>
      'Pesquise no LRCLIB por música e artista e escolha a versão correta.';

  @override
  String get visualFollowTheme => 'Seguir tema';

  @override
  String get visualFooterLyrics => 'Mostrar letras no rodapé';

  @override
  String get visualFooterTips => 'Mostrar dicas no rodapé';

  @override
  String get visualFooterTips1 =>
      'Sem pressa. Dê um pouco de tempo à curiosidade.';

  @override
  String get visualFooterTips10 =>
      'Não precisa preencher cada minuto. Deixe um pouco de espaço.';

  @override
  String get visualFooterTips2 =>
      'Anote uma ideia. Você pode organizar depois.';

  @override
  String get visualFooterTips3 =>
      'Divida uma grande ideia em um pequeno passo para hoje.';

  @override
  String get visualFooterTips4 => 'Alongue-se um pouco e descanse os olhos.';

  @override
  String get visualFooterTips5 =>
      'Uma ideia pode ficar sem resposta por enquanto.';

  @override
  String get visualFooterTips6 =>
      'Algumas descobertas chegam quando você desacelera.';

  @override
  String get visualFooterTips7 =>
      'Guardar um detalhe é uma forma de nutrir uma ideia.';

  @override
  String get visualFooterTips8 =>
      'Uma nota de hoje pode ser o começo de amanhã.';

  @override
  String get visualFooterTips9 =>
      'Deixe a mente passear e volte ao que você gosta.';

  @override
  String get visualFrosting => 'Desfoque';

  @override
  String get visualGif => 'GIF animado';

  @override
  String get visualHexColor => 'Cor HEX';

  @override
  String get visualHexInvalid => 'Insira uma cor hexadecimal de seis dígitos.';

  @override
  String get visualImage => 'Imagem';

  @override
  String get visualImageDecodeFailure =>
      'Não é possível decodificar a imagem. Salve e abra em outro aplicativo.';

  @override
  String visualImageLoadFailure(String name) {
    return 'Não foi possível carregar a imagem: $name';
  }

  @override
  String visualImageNotImported(String name) {
    return '$name (imagem não importada)';
  }

  @override
  String visualImageUnavailable(String name) {
    return 'Imagem indisponível: $name';
  }

  @override
  String get visualImportFailure =>
      'A importação falhou. Verifique arquivo, codificação e espaço disponível.';

  @override
  String get visualImportLyrics => 'Importar letras';

  @override
  String get visualImportMusic => 'Importar música';

  @override
  String get visualImportMusicHint =>
      'Selecione + para importar músicas locais';

  @override
  String get visualIndependentMaterial => 'Material personalizado';

  @override
  String get visualInheritColor => 'Usar tonalidade do tema';

  @override
  String get visualLinkFailure =>
      'Não foi possível abrir o link. Copie o endereço e tente novamente.';

  @override
  String visualLoadImage(String name) {
    return 'Carregar imagem · $name';
  }

  @override
  String get visualLoading => 'Carregando…';

  @override
  String get visualLyricsEmpty => 'O arquivo de letras está vazio.';

  @override
  String get visualLyricsFile => 'Arquivo de letras';

  @override
  String get visualLyricsImportHint =>
      'Importe um arquivo de letras ou pesquise online.';

  @override
  String get visualLyricsLoading => 'Carregando letras…';

  @override
  String visualLyricsMatch(String album, String kind, int seconds) {
    String _temp0 = intl.Intl.pluralLogic(
      seconds,
      locale: localeName,
      other: '$seconds segundos',
      one: '$seconds segundo',
    );
    return '$album\n$kind · $_temp0';
  }

  @override
  String get visualLyricsMissing =>
      'Letras não encontradas. Importe um arquivo ou pesquise novamente.';

  @override
  String get visualLyricsNotFound =>
      'Nenhuma letra encontrada. Tente outro título ou artista.';

  @override
  String get visualLyricsOnPlay => 'Carregar letras durante a reprodução';

  @override
  String get visualLyricsParseFailure =>
      'Não foi possível interpretar as letras. Importe novamente.';

  @override
  String get visualLyricsReadFailure =>
      'Não foi possível carregar as letras. Importe manualmente ou tente novamente.';

  @override
  String get visualLyricsServiceFailure =>
      'Não foi possível conectar ao serviço de letras. Tente mais tarde ou importe letras locais.';

  @override
  String get visualLyricsSize => 'O arquivo de letras deve ter no máximo 1 MB.';

  @override
  String get visualLyricsSources => 'Arquivo local → Incorporadas → LRCLIB';

  @override
  String get visualLyricsVersions =>
      'Várias versões encontradas. Escolha uma na pesquisa.';

  @override
  String get visualMaterialPreview => 'Prévia do material';

  @override
  String get visualMaximize => 'Maximizar';

  @override
  String get visualMediaAddress => 'Endereço da mídia';

  @override
  String get visualMediaAddressInvalid =>
      'Insira um endereço HTTP ou HTTPS válido sem dados de login.';

  @override
  String get visualMediaPreviewFailure =>
      'Não é possível pré-visualizar esta mídia. Salve e abra em outro aplicativo.';

  @override
  String get visualMediaType => 'Tipo de mídia';

  @override
  String get visualMinimize => 'Minimizar';

  @override
  String get visualMusic => 'Música';

  @override
  String get visualMusicEmptyTitle => 'Dê espaço à música';

  @override
  String get visualMusicPlayer => 'Reprodutor de música';

  @override
  String get visualNextTrack => 'Próxima faixa';

  @override
  String get visualNoLyricsRead => 'Nenhuma letra carregada';

  @override
  String visualNoLyricsTitle(String title) {
    return '♪ $title · Sem letras';
  }

  @override
  String get visualNoTimeline => 'Sem dados de tempo';

  @override
  String get visualOpacity => 'Opacidade';

  @override
  String get visualOptionalArtist => 'Artista (opcional)';

  @override
  String get visualPauseMusic => 'Pausar música';

  @override
  String get visualPaused => 'Pausado';

  @override
  String get visualPlainLyrics => 'Letras em texto simples';

  @override
  String get visualPlayMusic => 'Reproduzir música';

  @override
  String get visualPlaybackFailure =>
      'Não é possível reproduzir a música. Verifique o arquivo ou tente outro formato de áudio.';

  @override
  String visualPlaybackPosition(int count, int index, String state) {
    return '$index / $count · $state';
  }

  @override
  String get visualPlaybackRequestFailure =>
      'Não foi possível iniciar a reprodução. Tente novamente.';

  @override
  String get visualPlaying => 'Reproduzindo';

  @override
  String get visualPlaylistEmpty => 'Sua lista está vazia';

  @override
  String get visualPlaylistLyricsHint =>
      'Importe letras LRC pelo menu da lista';

  @override
  String get visualPlaylistSaved =>
      'A lista e as letras são salvas automaticamente';

  @override
  String get visualPlaylistUpdateFailure =>
      'Não foi possível atualizar a lista. Tente novamente.';

  @override
  String get visualPreviewColor => 'Prévia da cor';

  @override
  String get visualPreviousTrack => 'Faixa anterior';

  @override
  String get visualRemoveAttachment => 'Remover anexo';

  @override
  String get visualRemoveTrack => 'Remover da lista';

  @override
  String get visualResetMaterial => 'Restaurar tema';

  @override
  String get visualRestoreWindow => 'Restaurar';

  @override
  String get visualSaveAttachment => 'Salvar anexo como';

  @override
  String get visualSearch => 'Pesquisar';

  @override
  String get visualSearchLyrics => 'Pesquisar letras';

  @override
  String get visualSongCover => 'Capa do álbum';

  @override
  String get visualSongTitle => 'Título da música';

  @override
  String get visualSyncedLyrics => 'Letras sincronizadas';

  @override
  String get visualTextureFailure =>
      'Não foi possível carregar a mídia. Verifique arquivo, endereço e formato. Na web, o acesso entre origens também deve ser permitido.';

  @override
  String get visualTextureLinkGuide =>
      'Cole um link HTTP ou HTTPS direto para imagem, GIF ou vídeo. Em páginas compartilhadas, encontre primeiro o endereço da mídia original.';

  @override
  String get visualTextureLinkTitle => 'Traga um pouco de inspiração';

  @override
  String get visualTexturePlaybackGuide =>
      'Os vídeos repetem sem som por padrão; ative o áudio nas configurações. Mídias online precisam permitir acesso, inclusive entre origens na web.';

  @override
  String get visualTipsMaterialGuide =>
      'Desativado: dicas transparentes; ativado: material abaixo. Seus valores são mantidos.';

  @override
  String get visualTransparentTips => 'Sobreposição transparente (padrão)';

  @override
  String get visualUseCustomMaterial => 'Usar material personalizado';

  @override
  String get visualVideo => 'Vídeo';

  @override
  String get visualViewLyrics => 'Ver letras';
}
