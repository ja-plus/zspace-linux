# 官方播放器实现结构（从符号表还原）

> 来源：`UZPlayer.exe` 里残留的 C++ 符号（Qt moc 元信息）。
> 生成命令：`python3 native/player/tools/extract_symbols.py <exe> <本文件>`
>
> 这份文档回答的是「官方有哪些类、每个类管什么」，
> 样式部分见 `PLAYER-FEATURE-PARITY.md`，素材见 `native/player/assets/README.md`。

识别到 **14** 个自有类、**285** 个方法。

## MainWindow（223 个方法）

**弹层**

- `BlurayListPopupSelectSlot`
- `CommonPopupItemDeleteSLot`
- `CommonPopupItemDeleteSlot`
- `CommonPopupItemSelectSLot`
- `CommonPopupItemSelectSlot`
- `LiveListPopupSelectSlot`
- `ScheduledTimerPopupCloseSlot`
- `ScheduledTimerPopupPlaySlot`
- `ScheduledTimerPopupSettingSlot`
- `SubtitlePopupAdjustSlot`
- `SubtitlePopupItemDeleteSlot`
- `SubtitlePopupItemSelectSlot`
- `VideoListPopupSelectSlot`
- `centerPlayerPopup`
- `fitPlayerPopupToClient`
- `showScheduledTimerPopup`

**协议回包分支**

- `TcpRecvAudio`
- `TcpRecvCookieInfo`
- `TcpRecvDelayCollection`
- `TcpRecvDelayPlayList`
- `TcpRecvDelaySubtitles`
- `TcpRecvDownloadPgsSubtitle`
- `TcpRecvError`
- `TcpRecvExit`
- `TcpRecvFormat`
- `TcpRecvIntroOutro`
- `TcpRecvMountFormat`
- `TcpRecvMsgInfo`
- `TcpRecvNewPlayList`
- `TcpRecvSubAdd`
- `TcpRecvSubAddLocal`
- `TcpRecvSubDel`
- `TcpRecvSubtitleDownload`
- `TcpRecvSubtitleNewSearch`
- `TcpRecvSubtitleZipShow`
- `TcpRecvSwitchPgsSubtitle`
- `TcpRecvUrlInfo`
- `TcpRecvVideoInfo`

**快捷键**

- `KeyPressAltD`
- `KeyPressCtrlEnter`
- `KeyPressCtrlM`
- `KeyPressCtrlMinus`
- `KeyPressCtrlPlus`
- `KeyPressCtrlX`
- `KeyPressDown`
- `KeyPressEnter`
- `KeyPressEsc`
- `KeyPressPageDown`
- `KeyPressPageUp`
- `KeyPressShiftMinus`
- `KeyPressShiftPlus`
- `KeyPressSpace`
- `KeyPressUp`
- `KeySeekPress`
- `KeySeekRelease`

**定时器**

- `TimerBeginsetTimeOut`
- `TimerGetSizeTimeOut`
- `TimerHeartTimeOut`
- `TimerHidectlTimeOut`
- `TimerMouseClickTimeOut`
- `TimerNetTimeOut`
- `TimerSeekTimeout`
- `TimerSetSubTimeOut`
- `TimerShowStepEnd`
- `TimerShowStepEndAndLoop`
- `TimerShowStepPreview`
- `TimerSubAddTimeOut`
- `TimerUpdateSubTimeOut`

**工具栏/快捷键槽**

- `BToolMouseMove`
- `BToolSeekTime`
- `BToolSetMute`
- `BToolShowVolume`
- `BToolbackVideo`
- `BToolnextVideo`
- `BToolpauseVideo`
- `BToolplayVideo`

**状态/显示**

- `ChangeFormatFlag`
- `ChangeStrmFormatFlag`
- `HideApp`
- `InitPlayerConfig`
- `InitResize`
- `InitTcp`
- `InitVlc`
- `InitVolume`
- `SetConfigSubtitle`
- `SetDefaultSubtitle`
- `SetPlayRate`
- `SetRecordSubtitle`
- `SetSubTitle`
- `ShowContinue`
- `ShowLoading`
- `ShowReplay`
- `ShowStepEnd`
- `ShowStepEndAndLoop`
- `ShowStepPreview`
- `ShowStepStart`
- `ShowToast`
- `ShowToastInfo`
- `ShowToastTimer`
- `ShowVolumePercent`
- `UpdateMediaConfig`
- `resetFormatSwitchState`

**其它**

- `ApplyVideo360Viewpoint`
- `CloseApp`
- `ConfigChangeChannelSlot`
- `ConfigChangeImageInfoSlot`
- `ConfigCloseSlot`
- `ConfigDecodeGpuSlot`
- `ConfigDecodeSoftSlot`
- `ConfigMouseMoveSlot`
- `ConfigPlayLoopChangeSlot`
- `ConfigSetShortcutEnableSlot`
- `ConfigSleepTimerChangeSlot`
- `ControlBarBackVideoSlot`
- `ControlBarMenu`
- `ControlBarMenuSlot`
- `ControlBarNextVideoSlot`
- `ControlBarPauseSlot`
- `ControlBarPlaySlot`
- `ControlbarEpisodeSlot`
- `ControlbarMore`
- `ControlbarMoreSlot`
- `ControlbarSliderRightPressSlot`
- `ControlbarSubtitleSignal`
- `ControlbarSubtitleSlot`
- `CopyVideoInfo`
- `DeinitVlc`
- `EofTipRetry`
- `Fullmized`
- `GeometryChanged`
- `InfoShowConfigSlot`
- `IntroOutroFloatingSlot`
- `IntroOutroSettingSlot`
- `MainWindow`
- `Maximized`
- `MinApp`
- `Normalmized`
- `PlaySeekImmediately`
- `PlayVideo`
- `PlayerAudioCodec`
- `PlayerAudioTrackAdd`
- `PlayerESSeletect`
- `PlayerError`
- `PlayerExit`
- `PlayerPercent`
- `PlayerPlayStopped`
- `PlayerPlayingEnd`
- `PlayerPlayingStart`
- `PlayerUpdateFullTime`
- `PlayerVideo360Changed`
- `PlayerVideoCodec`
- `PlayerVoutDemuxModuleName`
- `PlayerVoutModuleName`
- `SCChangeFormat`
- `SCChangeSubFile`
- `SCChangeSubtitle`
- `SCClose`
- `SCDecodeGpu`
- `SCDecodeSoft`
- `SCDelSubtitle`
- `SCInitConfigSub`
- `SCInitSub`
- `SCMouseMove`
- `SCSubSetTime`
- `SSAddSub`
- `SSDownload`
- `SSGetSub`
- `SSGetVideo`
- `SSNewSearch`
- `SSZipShow`
- `ScreenResolutionChanged`
- `SendSwitchSubtitle`
- `StopLoading`
- `StopReplay`
- `SubtitleSearchAddSubSlot`
- `SubtitleSearchDownloadSlot`
- `SubtitleSearchNewSearchSlot`
- `SubtitleSearchZipShowSlot`
- `SubtitleSetCfgSlot`
- `SubtitleSetTimeSlot`
- `TToolfrontWindow`
- `TTooltitlepress`
- `TTooltitlerelease`
- `TcpComplete`
- `TitleBarVideoInfo`
- `VBlurayListPlayVideo`
- `VListPlayVideo`
- `VlcComplete`
- `VolumeChange`
- `autoReplay`
- `changeEvent`
- `clearPlayerCache`
- `commitPendingKeySeek`
- `eventFilter`
- `g_vVideo`
- `get115FormatList`
- `getProxyFlag`
- `getVideoFormat`
- `incrementalUpdateSubtitles`
- `inheritSubtitles`
- `initData`
- `liveReconnect`
- `nativeEvent`
- `playLoop`
- `playerReport`
- `registerBossHotKey`
- `resizeEvent`
- `resumeScheduledTimerStop`
- `scheduleKeySeek`
- `scheduleLiveReconnect`
- `sendReportInfo`
- `setAudioTrack`
- `setVideoTitle`
- `shouldIgnoreKeySeekCallback`
- `sleepTimer`
- `startSleepTimer`
- `strmSwitchFormatPlay`
- `switchFormat`
- `switchLiveStreamByIndex`
- `syncPlayerControlGeometry`
- `updateStrmSubtitles`
- `updateSubtitles`
- `updateVideoList`

## TitleBar（9 个方法）

**控件槽**

- `on_btnExitMac_clicked`
- `on_btnExitWin_clicked`
- `on_btnFront_clicked`
- `on_btnInfo_clicked`
- `on_btnMaxMac_clicked`
- `on_btnMaxWin_clicked`
- `on_btnMinMac_clicked`
- `on_btnMinWin_clicked`
- `on_frontButton_clicked`

## SetCommon（8 个方法）

**控件槽**

- `on_btnAllPlay_clicked`
- `on_btnRandomPlay_clicked`
- `on_btnSeqPlay_clicked`
- `on_btnSinglePlay_clicked`
- `on_combScale_currentIndexChanged`
- `on_combSeek_currentIndexChanged`
- `on_combVolume_currentIndexChanged`

**状态/显示**

- `SetChangeIndex`

## VideoListPopup（8 个方法）

**状态/显示**

- `SetVideoList`

**其它**

- `EpisodeItemSelect`
- `EpisodeMenuSelect`
- `showFile`
- `showIntro`
- `showNumber`
- `sortAscending`
- `sortDescending`

## Vlc（8 个方法）

**其它**

- `Buffering`
- `Ended`
- `Error`
- `Idle`
- `Opening`
- `Paused`
- `Playing`
- `Stopped`

## ControlBar（5 个方法）

**控件槽**

- `on_btnFullScreen_clicked`
- `on_btnRestoreScreen_clicked`
- `on_btnRestoreSreen_clicked`
- `on_btnVolume_clicked`

**状态/显示**

- `SetFtimeText`

## HttpPlugin（5 个方法）

**状态/显示**

- `GetExternalSubtitleList`
- `GetSkipSetting`
- `GetSubtitleDownloadAll`
- `SetSkipSetting`

**其它**

- `DownloadAllSubtitles`

## LiveListPopup（4 个方法）

**其它**

- `onChannelListFinished`
- `onLogoFinished`
- `requestChannelList`
- `requestLogo`

## SubtitleSearchPopup（4 个方法）

**状态/显示**

- `SetNewSubAdd`
- `SetSubAdd`

**其它**

- `SubtitleSearchSelect`
- `SubtitleSearchZipShow`

## SubtitlePopup（3 个方法）

**其它**

- `SubtitleAddBtnLocal`
- `SubtitleAddBtnNas`
- `SubtitleAddBtnSearch`

## ConfigPopup（2 个方法）

**控件槽**

- `on_btnPicture_clicked`

**其它**

- `ResetSize`

## EpisodeItem（2 个方法）

**其它**

- `onDownloadFinished`
- `requestEpisodeIcon`

## SubtitleSearchList（2 个方法）

**状态/显示**

- `SetItemCheckResult`

**其它**

- `SelectItemZipShow`

## SubtitleSetPopup（2 个方法）

**状态/显示**

- `SetDelayTime`
- `SetSubtitleInfo`
