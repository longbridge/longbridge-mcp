# 热门金融/股票/交易终端开源项目汇总

> 收录 GitHub Stars 超过 2000 的热门金融开源项目（AI 分析类放宽至 500+），按分类整理。
> 数据更新时间：2026-06（持续补充中）
>
> **市场标记说明：** 🌍 全球多市场  🇨🇳 仅A股  🇨🇳+ A/H/港股  ⛓️ 加密/链上  ⚠️ 已归档停止维护

---

## 目录

- [一、交易终端](#一交易终端--trading-terminals)
- [二、量化回测框架](#二量化回测框架--quant-backtesting-frameworks)
- [三、股票市场数据](#三股票市场数据--market-data)
- [四、技术分析](#四技术分析--technical-analysis)
- [五、算法/自动化交易](#五算法自动化交易--algorithmic-trading)
- [六、投资组合与风险管理](#六投资组合与风险管理--portfolio--risk)
- [七、期权与衍生品](#七期权与衍生品--options--derivatives)
- [八、加密货币交易](#八加密货币交易--crypto-trading)
- [九、财务基本面分析](#九财务基本面分析--fundamental-analysis)
- [十、金融可视化](#十金融可视化--financial-visualization)
- [十一、精选资源列表](#十一精选资源列表--curated-lists)
- [十二、AI/LLM 金融应用](#十二aillm-金融应用--ai--llm-in-finance)
- [十三、Alpha 因子研究](#十三alpha-因子研究--factor-research)
- [十四、高频交易](#十四高频交易--high-frequency-trading)
- [十五、券商 API 接口](#十五券商-api-接口--broker-api-clients)
- [十六、个人财务管理](#十六个人财务管理--personal-finance)
- [十七、财富/投资组合追踪](#十七财富投资组合追踪--wealth--portfolio-tracking)
- [十八、机构量化工具](#十八机构量化工具--institutional-quant-tools)
- [十九、金融数据库](#十九金融数据库--financial-databases)
- [二十、Java 生态](#二十java-生态--java-ecosystem)
- [二十一、个人财务与会计](#二十一个人财务与会计--personal-finance--accounting)
- [二十二、Go 语言生态](#二十二go-语言生态--go-ecosystem)
- [二十三、AI 每日分析与自动化报告](#二十三ai-每日分析与自动化报告--ai-daily-analysis)
- [二十四、FIX 协议与撮合引擎](#二十四fix-协议与撮合引擎--fix-protocol--matching-engines)
- [二十五、加密货币数据 Feed](#二十五加密货币数据-feed--crypto-data-feeds)
- [二十六、DeFi 与链上开发](#二十六defi-与链上开发--defi--on-chain)
- [二十七、宏观经济数据](#二十七宏观经济数据--macro--economic-data)
- [二十八、券商 API 接口（补充）](#二十八券商-api-接口补充--broker-apis)
- [二十九、时序数据库](#二十九时序数据库--time-series-databases)
- [三十、Rust 生态](#三十rust-生态--rust-ecosystem)
- [三十一、ML 量化研究](#三十一ml-量化研究--ml-quant-research)
- [三十二、统计与计量经济学](#三十二统计与计量经济学--statistics--econometrics)
- [三十三、SEC 文件解析](#三十三sec-文件解析--sec-filing-tools)
- [三十四、高性能交易基础设施](#三十四高性能交易基础设施--hft-infrastructure)
- [三十五、支付与核心银行基础设施](#三十五支付与核心银行基础设施--payments--core-banking)
- [三十六、金融情绪分析](#三十六金融情绪分析--financial-sentiment)
- [三十七、企业财务与 ERP](#三十七企业财务与-erp--enterprise-finance--erp)

---

## 一、交易终端 / Trading Terminals

| 项目 | Stars | 语言 | 市场 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | ------ | --- | --- | -------- |
| **OpenBB** | ~69k | Python | 🌍 全球 | 需数据 | ★★★ | https://github.com/OpenBB-finance/OpenBB |
| **FinceptTerminal** | ~26k | Python/C++ | 🌍 全球 | 需数据 | ★★ | https://github.com/Fincept-Corporation/FinceptTerminal |
| **StockSharp** | ~7k | C# | 🌍 全球 | 需数据 | ★ | https://github.com/StockSharp/StockSharp |

**OpenBB**
Bloomberg Terminal 的开源替代品。集成 AI 驱动的投资研究功能，支持股票、ETF、加密货币、宏观经济数据等多数据源，可通过 MCP 或 Python SDK 扩展，是当前最完整的开源投资研究平台。

**FinceptTerminal**
Bloomberg Terminal 的开源桌面替代品，C++20 + Qt6 原生实现（非 Electron），提供股票行情、宏观经济、投资研究数据工具，2026 年因 GitHub Trending 爆红至 26k Stars。注意：核心功能已转向订阅制，部分功能受限。

**StockSharp**
企业级算法交易与量化交易开源平台（.NET 生态）。支持股票、外汇、加密货币和期权，提供图形化策略设计器，适合 C# 技术栈团队。

---

## 二、量化回测框架 / Quant Backtesting Frameworks

| 项目 | Stars | 语言 | 市场 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | ------ | --- | --- | -------- |
| **vnpy / vn.py** | ~25k | Python | 🌍 全球 | 需数据 | ★★★ | https://github.com/vnpy/vnpy |
| **backtrader** | ~22k | Python | 🌍 全球 | 需数据 | ★★ | https://github.com/mementum/backtrader |
| **zipline** | ~20k | Python | 🌍 全球 | 需数据 | ★ | https://github.com/quantopian/zipline |
| **abu（阿布量化）** | ~12k | Python | 🇨🇳+ A/美/期货/BTC | 需数据 | ★★ | https://github.com/bbfamily/abu |
| **Lean (QuantConnect)** | ~10k | C#/Python | 🌍 全球 | 需数据 | ★★ | https://github.com/QuantConnect/Lean |
| **backtesting.py** | ~8k | Python | 🌍 全球 | 需数据 | ★ | https://github.com/kernc/backtesting.py |
| **rqalpha** | ~6.4k | Python | 🇨🇳 仅A股 | 需数据 | ★★ | https://github.com/ricequant/rqalpha |
| **WonderTrader** | ~6.2k | C++/Python | 🇨🇳 国内市场 | 需数据 | ★★ | https://github.com/wondertrader/wondertrader |
| **PyAlgoTrade** | ~4.6k | Python | 🌍 全球 | 需数据 | ★ | https://github.com/gbeced/pyalgotrade |
| **zvt** | ~4.2k | Python | 🇨🇳 国内市场 | 需数据 | ★★ | https://github.com/zvtvz/zvt |
| **finmarketpy** | ~3.7k | Python | 🌍 全球 | 需数据 | ★ | https://github.com/cuemacro/finmarketpy |
| **pybroker** | ~3.4k | Python | 🌍 全球 | 需数据 | ★ | https://github.com/edtechre/pybroker |
| **vectorbt** | ~4k | Python | 🌍 全球 | 需数据 | ★★ | https://github.com/polakowo/vectorbt |
| **nautilus_trader** | ~4k | Python/Rust | 🌍 全球 | 需数据 | ★★★ | https://github.com/nautechsystems/nautilus_trader |
| **pysystemtrade** | ~3k | Python | 🌍 全球 | 需数据 | ★ | https://github.com/pst-group/pysystemtrade |
| **TradeMaster** | ~2.8k | Python | 🌍 全球 | 需数据 | ★ | https://github.com/TradeMaster-NTU/TradeMaster |
| **bt** | ~2.7k | Python | 🌍 全球 | 需数据 | — | https://github.com/pmorissette/bt |
| **ffn** | ~2.4k | Python | 🌍 全球 | 需数据 | — | https://github.com/pmorissette/ffn |

> 🇨🇳 仅A股：框架/数据仅支持中国A股；🇨🇳 国内市场：主要对接A股+国内期货/期权，不支持国际市场；🌍 全球：支持多国际市场。

**vnpy / vn.py**
国内最流行的开源量化交易平台。支持 CTP、XTP、Interactive Brokers 等国内外多种交易接口，社区活跃，适合国内量化团队。

**backtrader**
功能完整的 Python 回测框架，事件驱动架构，内置 150+ 技术指标，支持多数据源和实时交易对接。入门友好，文档详尽。

**zipline**
Quantopian 出品的 Pythonic 算法交易库。事件驱动架构，与 pyfolio/alphalens 配合使用，适合学术研究和策略研究。（Quantopian 已关闭，由社区维护）

**abu（阿布量化）**
国产开源量化交易平台，支持 A 股、美股、期货、比特币，深度整合机器学习（自动策略优化、交易行为分析），配套《量化交易如何寻找标的》书籍。

**Lean (QuantConnect)**
QuantConnect 云平台背后的开源引擎。支持股票、期权、期货、外汇、加密货币的回测与实盘，多语言接口（C#/Python）。

**backtesting.py**
基于 Pandas/NumPy/Bokeh 的简洁 Python 回测库，API 极简，支持 TA-Lib、pandas-ta 等指标库，结果以可交互 Bokeh 图表呈现，适合快速验证策略原型。

**rqalpha** 🇨🇳 仅A股
米宽科技开源的 Python 算法回测框架，仅支持 A 股股票、A 股期货、A 股期权等国内证券类型，不支持港股/美股/国际期货。

**WonderTrader** 🇨🇳 国内市场
国产开源量化研发交易一站式框架，C++ 高性能引擎，主要对接 CTP（期货）、XTP（A股）等国内交易接口，支持 CTA、HFT、期权等多策略类型，不支持国际市场。

**PyAlgoTrade** ⚠️ 不再活跃维护
事件驱动的 Python 量化回测库，支持纸上交易和实盘，曾是最流行的开源回测框架之一，现已不再活跃维护，建议新项目选择 backtrader 或 backtesting.py 替代。

**finmarketpy**
Saeed Amen 出品的量化交易回测与市场分析 Python 库，支持 Bloomberg/Quandl/Yahoo Finance 数据源，与 chartpy、findatapy 配套使用，设计面向专业外汇和宏观量化研究。

**pybroker**
面向机器学习策略的现代算法交易回测框架，NumPy/Numba 加速，内置 Walkforward 验证和 Bootstrap 统计检验，支持原生集成 scikit-learn / PyTorch 等 ML 模型训练流程。

**zvt** 🇨🇳 国内市场
模块化量化框架，数据源以 A 股、港股为主，不支持国际市场。用统一方式处理数据记录、因子计算、证券选择、回测和实时交易，提供实时图表展示，设计理念清晰，二次开发友好。

**vectorbt**
基于 NumPy/Numba 的向量化回测引擎。可在秒级完成数千策略的参数优化扫描，适合需要大规模参数寻优的量化研究。

**nautilus_trader**
Rust 加速核心 + Python 接口的高性能算法交易平台。延迟极低，适合专业量化团队的生产级部署。

**pysystemtrade**
《系统化交易》作者 Robert Carver 开源的系统化期货交易引擎，覆盖从策略研究到 Interactive Brokers 实盘的完整流程，是期货系统化交易的权威参考实现。

**bt**
基于 ffn 的灵活 Python 回测框架，通过可组合的 Algo 模块快速构建和测试交易策略，代码简洁，适合快速原型验证。

**ffn**
量化金融 Python 函数库，提供业绩评估、数据转换、图表绘制等常用工具，是 bt 的底层依赖，也可单独用于绩效分析。

**TradeMaster**
南洋理工大学出品，专注于强化学习（RL）在量化交易中的应用，内置多种 RL 算法和金融环境。

---

## 三、股票市场数据 / Market Data

| 项目 | Stars | 语言 | 市场 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | ------ | --- | --- | -------- |
| **yfinance** | ~17k | Python | 🌍 全球 | 需数据 | ★★ | https://github.com/ranaroussi/yfinance |
| **tushare** | ~13k | Python | 🇨🇳 仅A股 | 需数据 | ★★ | https://github.com/waditu/tushare |
| **AKShare** | ~11.6k | Python | 🇨🇳+ A/H/美股 | 需数据 | ★★ | https://github.com/akfamily/akshare |
| **alpha_vantage** | ~4.5k | Python | 🌍 全球 | 需数据 | ★ | https://github.com/RomelTorres/alpha_vantage |
| **efinance** | ~3.8k | Python | 🇨🇳 仅A股 | 需数据 | ★★ | https://github.com/1quant/efinance |
| **adata** | ~2.1k | Python | 🇨🇳 仅A股 | 需数据 | ★★ | https://github.com/1nchaos/adata |

**yfinance**
从 Yahoo Finance 下载历史和实时行情数据的事实标准 Python 库。简单易用，支持股票、ETF、期权链、宏观数据。

**tushare** 🇨🇳 仅A股
国内最早的 A 股数据工具，现已升级为专业数据平台 Tushare Pro。免费版提供基础行情，付费版覆盖更多数据。数据范围以 A 股为主，不支持国际市场。

**AKShare**
完全免费的中国股票/港股/美股财经数据接口库。聚合新浪、东方财富、雪球等数十个数据源，覆盖面极广。

**alpha_vantage**
Alpha Vantage 免费金融数据 API 的 Python 封装，支持全球股票实时/历史行情、150+ 技术指标、外汇汇率、加密货币，免费 API Key 申请即用，返回 JSON 或 Pandas DataFrame。

**efinance** 🇨🇳 仅A股
极简风格的 A 股/基金/债券/期货数据获取库，底层对接东方财富数据源，API 设计简洁，适合快速获取行情数据用于量化回测，不支持国际市场。

**adata** 🇨🇳 仅A股
免费开源 A 股量化数据库，多数据源融合 + 动态代理保障高可用，覆盖行情、K 线、概念板块、交易日历、财务数据，专为个人量化研究设计，不支持国际市场。

---

## 四、技术分析 / Technical Analysis

| 项目 | Stars | 语言 | 市场 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | ------ | --- | --- | -------- |
| **TA-Lib (Python)** | ~10k | Python/C | 🌍 全球 | 自包含 | ★ | https://github.com/TA-Lib/ta-lib-python |
| **pandas-ta** | ~5k | Python | 🌍 全球 | 自包含 | ★ | https://github.com/twopirllc/pandas-ta |
| **ta** | ~4k | Python | 🌍 全球 | 自包含 | ★ | https://github.com/bukosabino/ta |

**TA-Lib (Python)**
业界标准 TA-Lib 的 Python 封装，提供 150+ 技术指标（MA、RSI、MACD、布林带等），底层 C 实现性能极高。

**pandas-ta**
纯 Python 实现的技术分析库，130+ 指标，无需安装 TA-Lib 即可运行。支持 Pandas DataFrame 链式调用，API 简洁。

**ta**
基于 Pandas/NumPy 的轻量级技术分析库，覆盖趋势、动量、波动率、成交量 4 大类指标，代码简洁易读。

---

## 五、算法/自动化交易 / Algorithmic Trading

| 项目 | Stars | 语言 | 市场 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | ------ | --- | --- | -------- |
| **freqtrade** | ~50k | Python | ⛓️ 加密 | 需数据 | ★ | https://github.com/freqtrade/freqtrade |
| **CCXT** | ~42k | JS/Python/PHP | ⛓️ 加密 | 需数据 | ★ | https://github.com/ccxt/ccxt |
| **Hummingbot** | ~19k | Python | ⛓️ 加密 | 需数据 | ★ | https://github.com/hummingbot/hummingbot |
| **Qlib (Microsoft)** | ~18k | Python | 🌍 全球 | 需数据 | ★★ | https://github.com/microsoft/qlib |
| **FinRL** | ~11.5k | Python | 🌍 全球 | 需数据 | ★ | https://github.com/AI4Finance-Foundation/FinRL |
| **Superalgos** | ~5.5k | JavaScript | ⛓️ 加密 | 需数据 | ★ | https://github.com/Superalgos/Superalgos |
| **catalyst (enigmampc)** | ~2.4k | Python | ⛓️ 加密 | 需数据 | — | https://github.com/enigmampc/catalyst |
| **jesse** | ~6.5k | Python | ⛓️ 加密 | 需数据 | ★ | https://github.com/jesse-ai/jesse |
| **OctoBot** | ~3.5k | Python | ⛓️ 加密 | 需数据 | ★ | https://github.com/Drakkar-Software/OctoBot |

**freqtrade**
最流行的开源加密货币交易机器人框架。支持 AI/ML 策略优化、超参数搜索、Web UI 实时监控，社区策略丰富。

**CCXT**
统一的加密货币交易所 API 库，支持 100+ 交易所（Binance、OKX、Bybit 等），Python/JavaScript/PHP 三种语言。

**Hummingbot**
专注做市（Market Making）和套利策略的高频交易框架，支持 50+ CEX/DEX 交易所。

**Qlib (Microsoft)**
微软出品的 AI 量化投资平台，提供从数据处理、Alpha 因子挖掘、模型训练到策略回测的端到端 ML 流水线。

**FinRL**
首个将深度强化学习（DRL）系统化应用于量化金融的开源框架，覆盖股票、期货、加密货币等多资产场景。

**jesse**
专为加密货币设计的简洁交易框架，内置 AI 助手辅助策略调试，回测结果可直接转为实盘信号。

**Superalgos**
免费开源的可视化加密货币交易机器人平台，无需编写代码即可通过节点画布设计策略，内置图表分析、数据挖掘、回测和多服务器分布式部署能力，社区共享策略生态活跃。

**catalyst (enigmampc)** ⚠️ 已归档
基于 Zipline 的加密货币算法交易库，支持 Binance/Bitfinex 等多交易所回测和实盘，曾是加密量化领域最受欢迎的 Python 框架之一，2022 年已归档停止维护。

**OctoBot**
高度可扩展的加密货币交易机器人，插件架构支持自定义策略和技术分析模块。

---

## 六、投资组合与风险管理 / Portfolio & Risk

| 项目 | Stars | 语言 | 市场 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | ------ | --- | --- | -------- |
| **PyPortfolioOpt** | ~5.8k | Python | 🌍 全球 | 可选 | ★★ | https://github.com/robertmartin8/PyPortfolioOpt |
| **pyfolio** | ~5k | Python | 🌍 全球 | 需数据 | ★★ | https://github.com/quantopian/pyfolio |
| **QuantStats** | ~4k | Python | 🌍 全球 | 需数据 | ★★ | https://github.com/ranaroussi/quantstats |
| **Riskfolio-Lib** | ~3k | Python | 🌍 全球 | 可选 | ★★ | https://github.com/dcajasn/Riskfolio-Lib |

**PyPortfolioOpt**
投资组合优化库，实现均值方差（Markowitz）、Black-Litterman、层次风险平价（HRP）等主流优化模型。

**pyfolio**
Quantopian 出品的投资组合分析利器，生成"撕裂报告"（Tear Sheet），覆盖收益、回撤、因子归因、交易分析等维度。

**QuantStats**
一键生成含 Sharpe/Sortino/Calmar 等核心指标的投资组合分析 HTML 报告，适合策略汇报和客户展示。

**Riskfolio-Lib**
专业投资组合优化库，支持 26 种凸风险度量（CVaR、CDaR、Omega 等）和 Black-Litterman、因子模型等高级方法。

---

## 七、期权与衍生品 / Options & Derivatives

| 项目 | Stars | 语言 | 市场 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | ------ | --- | --- | -------- |
| **QuantLib** | ~5k | C++ | 🌍 全球 | 自包含 | — | https://github.com/lballabio/QuantLib |
| **tf-quant-finance** | ~5k | Python | 🌍 全球 | 自包含 | — | https://github.com/google/tf-quant-finance |
| **QuantLib-SWIG** | ~2k | Python/Java/R | 🌍 全球 | 自包含 | — | https://github.com/lballabio/QuantLib-SWIG |

**QuantLib**
量化金融领域最权威的开源建模库（C++）。覆盖期权定价（Black-Scholes、Heston 等）、利率模型（Hull-White、LMM）、信用风险、固定收益等，被金融机构广泛用于定价引擎。

**tf-quant-finance**
Google 出品的高性能 TensorFlow 量化金融库，覆盖期权定价（Black-Scholes/Heston）、利率模型（Hull-White/SABR/HJM）、Monte Carlo 路径生成和自动微分风险计算，支持 GPU 批量加速定价，适合需要大规模并行计算的衍生品定价场景。

**QuantLib-SWIG**
QuantLib 的多语言绑定，提供 Python、Java、R、C# 等语言接口，让非 C++ 用户也能访问 QuantLib 全部功能。

---

## 八、加密货币交易 / Crypto Trading

| 项目 | Stars | 语言 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | --- | --- | -------- |
| **freqtrade** | ~50k | Python | 需数据 | ★ | https://github.com/freqtrade/freqtrade |
| **CCXT** | ~42k | JS/Python/PHP | 需数据 | ★ | https://github.com/ccxt/ccxt |
| **Hummingbot** | ~19k | Python | 需数据 | ★ | https://github.com/hummingbot/hummingbot |
| **jesse** | ~6.5k | Python | 需数据 | ★ | https://github.com/jesse-ai/jesse |
| **OctoBot** | ~3.5k | Python | 需数据 | ★ | https://github.com/Drakkar-Software/OctoBot |

> 以上项目均在"算法交易"分类中有详细介绍，加密货币方向是其主要应用场景。

---

## 九、财务基本面分析 / Fundamental Analysis

| 项目 | Stars | 语言 | 市场 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | ------ | --- | --- | -------- |
| **OpenBB** | ~69k | Python | 🌍 全球 | 需数据 | ★★★ | https://github.com/OpenBB-finance/OpenBB |
| **machine-learning-for-trading** | ~17k | Python | 🌍 全球 | 需数据 | ★★ | https://github.com/stefan-jansen/machine-learning-for-trading |
| **financepy** | ~2k | Python | 🌍 全球 | 自包含 | — | https://github.com/domokane/FinancePy |

**machine-learning-for-trading**
《机器学习与算法交易》（Stefan Jansen 著）配套代码仓库。覆盖因子挖掘、NLP 情绪分析、深度学习预测等 ML 在量化中的完整实践，是学习量化 ML 的最佳资源之一。

**financepy**
纯 Python 实现的金融产品定价库，覆盖固定收益、股权衍生品、外汇、信用产品，代码结构清晰，适合学习金融工程原理。

---

## 十、金融可视化 / Financial Visualization

| 项目 | Stars | 语言 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | --- | --- | -------- |
| **D3.js** | ~112k | JavaScript | 自包含 | — | https://github.com/d3/d3 |
| **Apache Superset** | ~70k | Python/TS | 自包含 | ★ | https://github.com/apache/superset |
| **Grafana** | ~74.7k | TypeScript/Go | 自包含 | ★ | https://github.com/grafana/grafana |
| **Apache ECharts** | ~66.6k | TypeScript | 自包含 | ★★ | https://github.com/apache/echarts |
| **Metabase** | ~40k | Clojure/JS | 自包含 | — | https://github.com/metabase/metabase |
| **Redash** | ~28k | Python/React | 自包含 | — | https://github.com/getredash/redash |
| **TanStack Table** | ~28.1k | TypeScript | 自包含 | — | https://github.com/tanstack/table |
| **Recharts** | ~27.3k | JavaScript | 自包含 | ★ | https://github.com/recharts/recharts |
| **Plotly Dash** | ~24.3k | Python | 可选 | ★ | https://github.com/plotly/dash |
| **bokeh** | ~19.7k | Python/JS | 可选 | ★ | https://github.com/bokeh/bokeh |
| **pyecharts** | ~15.8k | Python | 可选 | ★★ | https://github.com/pyecharts/pyecharts |
| **lightweight-charts** | ~15k | TypeScript | 自包含 | ★★ | https://github.com/tradingview/lightweight-charts |
| **ApexCharts** | ~15.1k | JavaScript | 自包含 | ★ | https://github.com/apexcharts/apexcharts.js |
| **nivo** | ~13.6k | JavaScript | 自包含 | — | https://github.com/plouc/nivo |
| **Perspective (FINOS)** | ~10.4k | TS/Rust/C++ | 自包含 | ★ | https://github.com/finos/perspective |
| **AG Grid** | ~10.4k | TypeScript | 自包含 | ★ | https://github.com/ag-grid/ag-grid |
| **uPlot** | ~10.2k | JavaScript | 自包含 | ★ | https://github.com/leeoniya/uplot |
| **Panel (HoloViz)** | ~5.1k | Python | 可选 | ★ | https://github.com/holoviz/panel |
| **Vega-Lite** | ~5.3k | TypeScript | 自包含 | — | https://github.com/vega/vega-lite |
| **Observable Plot** | ~4.8k | JavaScript | 自包含 | — | https://github.com/observablehq/plot |
| **KLineChart** | ~3.9k | TypeScript | 自包含 | ★★ | https://github.com/klinecharts/KLineChart |
| **react-stockcharts** | ~4k | JavaScript | 自包含 | — | https://github.com/rrag/react-stockcharts |
| **mplfinance** | ~4k | Python | 自包含 | ★ | https://github.com/matplotlib/mplfinance |
| **trading-vue-js** | ~2.2k | Vue | 自包含 | — | https://github.com/tvjsx/trading-vue-js |
| **finplot** | ~2k | Python | 自包含 | ★ | https://github.com/highfellow/finplot |

**D3.js**
数据可视化领域最基础的底层 JavaScript 库，绝大多数金融图表库（recharts/nivo/lightweight-charts/ECharts）均构建于其之上，直接使用可实现任意自定义金融可视化效果，是理解金融图表渲染原理的必学工具。

**Apache Superset**
最流行的开源 BI 平台，连接数十种数据库（PostgreSQL/TimescaleDB/ClickHouse 等），广泛用于企业金融数据看板、交易数据探索和风控报表构建，功能接近 Tableau，适合量化团队自建内部数据平台。

**Metabase**
零代码 BI 工具，非技术人员可直接在数据库上构建金融报表和仪表盘，是中小型券商/基金内部 BI 的常用选择，支持自动问答式数据查询。

**Redash**
开源数据查询与可视化平台，支持 SQL 驱动的金融数据报表和定时刷新，适合量化团队自建内部数据工具和监控大盘。

**TanStack Table**
无头（Headless）数据表格引擎，不绑定 UI 框架，专为构建高性能持仓表、订单簿、交易记录等复杂金融数据表格而设计，支持虚拟滚动、排序、筛选，React/Vue/Solid 均支持。

**Recharts**
基于 React + D3 的声明式图表库，是金融 Web Dashboard 中折线图/柱状图/面积图的最常用选择，API 友好，npm 周下载量约 5000 万次。

**ApexCharts**
交互式 SVG 图表库，内置蜡烛图（candlestick）和 OHLC 图类型，React/Vue/Angular 均有官方封装，适合构建轻量级金融 Web 前端，无需依赖重量级图表库。

**nivo**
基于 D3 的 React 数据可视化库，支持 SVG/Canvas/HTML 多渲染器，开箱即用的精美视觉风格，适合金融分析报告前端和投资组合可视化。

**Perspective (FINOS)**
FINOS 金融开源基金会出品的流式分析组件，WebAssembly + C++ 核心，专为大规模实时金融数据集（tick 数据/持仓/订单流）设计，可在浏览器中交互式分析百万行数据，是构建高性能交易终端前端的利器。

**AG Grid**
企业级 JavaScript 数据表格，支持百万行虚拟滚动，广泛用于交易终端的持仓表、报价表、订单管理界面，React/Angular/Vue 均支持，是金融前端开发的事实标准网格组件。

**Vega-Lite**
基于 JSON 语法的声明式交互可视化语法（Vega 的高层封装），适合量化研究人员快速构建金融数据探索图表和研究报告可视化，与 Altair（Python 封装）配合使用效果极佳。

**Observable Plot**
D3 团队出品的探索性数据可视化库，语法简洁直观，适合量化研究人员在 Observable Notebook 中快速可视化金融时序数据和因子分布。

**trading-vue-js** ⚠️ 已归档
专为交易者设计的 Vue K 线图组件，支持在蜡烛图上叠加任意自定义图层（指标/标注/策略信号），已停止维护，仍可参考其架构思路。

**Grafana**
开源可观测性与数据可视化平台，支持 InfluxDB/Prometheus/PostgreSQL/TimescaleDB 等数十种数据源，常用于构建金融 tick 数据监控大盘和交易系统实时看板，插件生态极为丰富。

**Apache ECharts**
Apache 孵化的高性能交互式数据可视化库，内置 K 线图（candlestick）、散点图、热力图等金融图表类型，是国内金融前端的主流图表引擎，支持 Canvas/SVG 双渲染器。

**Plotly Dash**
Python 数据应用与交互式 Dashboard 框架，无需编写 JavaScript 即可构建金融分析 Web App，支持 Plotly 全系列图表，是量化研究结果展示和数据产品原型的主流工具。

**bokeh**
面向现代浏览器的 Python 交互式可视化库，支持流式大数据渲染和实时数据推送，是 backtesting.py 策略结果的默认可视化后端，适合构建金融数据探索工具。

**pyecharts**
ECharts 的 Python 封装库，支持 K 线/蜡烛图等金融图表类型，可在 Jupyter Notebook 中内联渲染，适合 A 股数据分析和量化研究报告生成场景。

**lightweight-charts**
TradingView 出品的高性能前端金融图表库。仅 45KB（gzip），基于 HTML5 Canvas 渲染，支持 K 线、面积图、柱状图，帧率极高，是构建 Web 交易终端的首选图表库。

**uPlot**
极小极快的时序/OHLC 图表库（~50KB，零依赖），渲染速度是 Chart.js/Highcharts 的数倍，适合高频 tick 数据实时展示和大数据量金融时序可视化。

**Panel (HoloViz)**
HoloViz 生态的 Python Dashboard 框架，可将任何可视化库（Bokeh/Plotly/Matplotlib/ECharts）打包为独立 Web App，适合量化研究成果的快速展示和内部工具构建。

**KLineChart**
高度可定制的轻量级 K 线图库（TypeScript），零依赖，40KB gzip，支持移动端触控，内置技术指标和绘图工具，是 lightweight-charts 的国产替代选择，API 更灵活。

**react-stockcharts** ⚠️ 已归档
基于 React + D3 的高度可定制 K 线图组件库，曾是 React 生态中最流行的金融图表库，现已停止维护，react-financial-charts 是其 TypeScript 社区继任者。

**mplfinance**
Matplotlib 官方维护的金融数据可视化扩展。支持 K 线图、OHLCV 柱状图、均线叠加，与 Pandas DataFrame 无缝集成。

**finplot**
专为金融数据设计的高性能 Python 绘图库，基于 PyQtGraph，低延迟实时渲染，适合构建本地桌面交易分析工具。

---

## 十一、精选资源列表 / Curated Lists

| 项目 | Stars | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | --- | --- | -------- |
| **awesome-quant** | ~20k | 自包含 | — | https://github.com/wilsonfreitas/awesome-quant |
| **awesome-crypto-trading-bots** | ~2.1k | 自包含 | — | https://github.com/botcrypto-io/awesome-crypto-trading-bots |
| **best-of-crypto** | ~2k | 自包含 | — | https://github.com/LukasMasuch/best-of-crypto |
| **financial-machine-learning** | ~8.7k | 自包含 | — | https://github.com/firmai/financial-machine-learning |
| **awesome-systematic-trading (paperswithbacktest)** | ~8.2k | 自包含 | — | https://github.com/paperswithbacktest/awesome-systematic-trading |
| **awesome-systematic-trading (wangzhe3224)** | ~8.4k | 自包含 | — | | https://github.com/wangzhe3224/awesome-systematic-trading |
| **awesome-ai-in-finance** | ~3.5k | 自包含 | — | https://github.com/georgezouq/awesome-ai-in-finance |
| **EliteQuant** | ~3.8k | 自包含 | — | https://github.com/EliteQuant/EliteQuant |
| **best-of-algorithmic-trading** | ~3k | 自包含 | — | https://github.com/merovinh/best-of-algorithmic-trading |

**awesome-quant**
量化金融领域最全面的开源资源汇总列表。按语言（Python/R/Julia/Matlab/C++）分类，涵盖数据、回测、ML、风险管理、研究论文等，是寻找量化工具的第一入口。

**awesome-systematic-trading**
系统化交易精选资源，涵盖加密货币、股票、期货、外汇等资产类别，包含论文、书籍、工具、数据源推荐。

**awesome-ai-in-finance**
精选 LLM 与深度学习在金融市场应用的资源列表，覆盖 AI Agent 交易、策略、数据源、NLP 情绪分析等方向，跟踪金融 AI 前沿进展的必备列表。

**financial-machine-learning (firmai)**
Derek Snow（NYU 教授）整理的实用金融机器学习工具和应用精选列表，每日更新，覆盖替代数据、NLP/文本分析、深度学习预测、固定收益、投资组合优化等多个子领域，是量化 ML 从业者的必备导航。

**awesome-systematic-trading (paperswithbacktest)**
收录 97 个量化库和 40+ 经学术机构验证策略（含 Sharpe 排序）的精选列表，配套回测代码。与 wangzhe3224 版本同名但内容不同，更侧重有回测验证的学术策略。

**awesome-systematic-trading (wangzhe3224)**
系统化交易精选资源，涵盖加密货币、股票、期货、外汇等资产类别，包含论文、书籍、工具、数据源推荐。

**EliteQuant**
量化建模、交易和组合管理的在线资源精选列表，按主题分类覆盖强化学习/深度学习/优化/数据源等方向，适合量化从业者系统性学习。

**awesome-crypto-trading-bots**
加密货币交易机器人精选列表，涵盖开源交易机器人、技术分析库、数据提供商，每周更新，是寻找加密自动交易工具的专项导航。

**best-of-crypto**
包含 3100+ 开源加密项目（总计 120 万+ Stars）的每周更新排行榜，按分类和质量评分排序，是全面了解加密开源生态的最佳入口。

**best-of-algorithmic-trading**
每周自动更新的算法交易开源项目排行榜，追踪 110+ 项目、310k+ 综合 Stars，按热度和类别排序。

---

## 十二、AI/LLM 金融应用 / AI & LLM in Finance

| 项目 | Stars | 语言 | 市场 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | ------ | --- | --- | -------- |
| **ai-hedge-fund** | ~60.5k | Python | 🌍 全球 | 需数据 | ★★★ | https://github.com/virattt/ai-hedge-fund |
| **TradingAgents** | ~87k | Python | 🌍 全球 | 需数据 | ★★★ | https://github.com/TauricResearch/TradingAgents |
| **stable-baselines3** | ~13.1k | Python | 🌍 全球 | 自包含 | — | https://github.com/DLR-RM/stable-baselines3 |
| **Qbot** | ~17.2k | Python | 🇨🇳+ 偏国内 | 需数据 | ★★ | https://github.com/UFund-Me/Qbot |
| **gym-anytrading** | ~2.3k | Python | 🌍 全球 | 需数据 | — | https://github.com/AminHP/gym-anytrading |
| **FinGPT** | ~20k | Python | 🌍 全球 | 需数据 | ★★★ | https://github.com/AI4Finance-Foundation/FinGPT |
| **FinRobot** | ~6.4k | Python | 🌍 全球 | 需数据 | ★★ | https://github.com/AI4Finance-Foundation/FinRobot |
| **FinBERT** | ~2.1k | Python | 🌍 全球 | 自包含 | — | https://github.com/ProsusAI/finBERT |

**ai-hedge-fund**
模拟顶级投资者（巴菲特、芒格、格雷厄姆、凯西·伍德、Michael Burry 等）作为 LLM Agent 协作分析股票的开源框架，包含基本面/技术面/风险/组合管理 Agent，是继 TradingAgents 之后爆红的 AI 金融 Agent 代表作，2025 年快速积累 60k+ Stars。

**TradingAgents**
2025 年爆红的多智能体 LLM 金融交易框架。模拟真实交易公司团队协作（基本面研究员、技术研究员、风险管理员、交易员），支持 GPT/Gemini/Claude 等主流模型，是当前金融 AI Agent 领域 Stars 增速最快的项目。

**FinGPT**
AI4Finance 基金会出品的开源金融大语言模型框架，提供数据流、微调流水线和多个下游任务（情绪分析、股价预测、研报生成等），被视为 BloombergGPT 的开源平民替代方案。

**FinRobot**
整合 LLM + 强化学习 + 量化分析的开源金融 AI Agent 平台，覆盖投资研究自动化、算法交易策略设计和风险评估，由 AI4Finance 基金会维护。

**stable-baselines3**
DLR（德国宇航中心）出品的 PyTorch 强化学习算法可靠实现集合（PPO/SAC/TD3/A2C/DDPG），单元测试覆盖率 95%，是 FinRL、gym-anytrading 等金融 RL 项目的标准算法后端，也是强化学习研究的基准工具库。

**gym-anytrading**
OpenAI Gym 兼容的金融交易强化学习环境，实现 ForexEnv/StocksEnv，提供结构化的外汇和股票 RL 训练场景，是金融强化学习入门的标准环境，与 stable-baselines3 配合使用效果最佳。

**Qbot** 🇨🇳 偏国内
AI 驱动的自动量化交易机器人，支持完全本地部署，整合强化学习/深度学习/传统量化策略，覆盖数据获取、回测到实盘全流程，主要面向 A 股市场，同时支持加密货币。

**FinBERT**
基于 BERT 在大规模金融文本上预训练的情绪分析模型（Prosus AI 出品），专为财经新闻、研究报告、电话会纪要的情绪打标而设计，是金融 NLP 领域的基础预训练模型之一。

---

## 十三、Alpha 因子研究 / Factor Research

| 项目 | Stars | 语言 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | --- | --- | -------- |
| **alphalens** | ~4.3k | Python | 需数据 | ★★ | https://github.com/quantopian/alphalens |

**alphalens**
Quantopian 出品的 Alpha 因子性能分析库。分析预测性因子的信息系数（IC）、IC 衰减、分组收益、换手率等关键指标，生成可视化因子分析报告。是因子研究的必备工具，常与 zipline + pyfolio 配合使用形成完整量化研究工具链。

---

## 十四、高频交易 / High-Frequency Trading

| 项目 | Stars | 语言 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | --- | --- | -------- |
| **hftbacktest** | ~4.2k | Python/Rust | 需数据 | ★★ | https://github.com/nkaz001/hftbacktest |

**hftbacktest**
开源高频交易与做市策略回测工具。利用完整 tick 数据（Level 2/3 订单簿 + 逐笔成交）进行精确模拟，考虑限价单队列位置与网络延迟，Rust 核心保证高性能，支持 Binance/Bybit 实盘对接示例。是目前开源 HFT 回测领域最严谨的工具。

---

## 十五、券商 API 接口 / Broker API Clients

| 项目 | Stars | 语言 | 市场 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | ------ | --- | --- | -------- |
| **ib_insync** | ~3.2k | Python | 🌍 全球 | 需数据 | ★ | https://github.com/erdewit/ib_insync |
| **robin_stocks** | ~2.1k | Python | 🌍 美股/加密 | 需数据 | ★ | https://github.com/jmfernandes/robin_stocks |

**ib_insync** ⚠️ 已归档
Interactive Brokers TWS/IB Gateway 的 Python 同步/异步封装框架，大幅简化原生 IB API 的复杂度，支持股票、期权、期货、外汇等多资产实盘交易。原作者 2024 年去世后已 archived，社区 fork `ib_async` 延续维护。

**robin_stocks**
Robinhood 券商的非官方 Python 接口库，支持股票、期权、加密货币的交易及持仓查询，是美国散户中使用最广的 Robinhood API 封装。

---

## 十六、个人财务管理 / Personal Finance

| 项目 | Stars | 语言 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | --- | --- | -------- |
| **maybe-finance/maybe** | ~44k | Ruby/Rails | 可选 | ★ | https://github.com/maybe-finance/maybe |
| **frappe/erpnext** | ~36.1k | Python | 自包含 | — | https://github.com/frappe/erpnext |
| **actualbudget/actual** | ~27.2k | TypeScript | 自包含 | — | https://github.com/actualbudget/actual |
| **Firefly III** | ~23.8k | PHP | 自包含 | — | https://github.com/firefly-iii/firefly-iii |
| **bigcapitalhq/bigcapital** | ~3.5k | TypeScript | 自包含 | — | https://github.com/bigcapitalhq/bigcapital |

**maybe-finance/maybe**
曾耗资 100 万美元开发、后完全开源的个人财务全功能平台。覆盖净值追踪、投资账户同步、预算、债务管理，Ruby on Rails + PostgreSQL，可完全自托管，在个人财务类开源项目中 Stars 排名第一。

**frappe/erpnext**
最流行的开源 ERP 系统，Python + Frappe 框架，内置完整会计模块（总账、应收/应付、多币种、财务报表），同时覆盖库存、采购、HR 管理，是 QuickBooks/SAP 的开源替代，适合中小企业。

**actualbudget/actual**
本地优先的个人预算 App，TypeScript 全栈，数据存储于本地并可加密同步，支持信封预算（Envelope Budgeting）方法，是 YNAB 的免费开源替代，隐私保护彻底。

**Firefly III**
最流行的自托管个人财务管理系统。支持收支记录、预算管理、账单跟踪、多账户多货币、数据导入导出，提供完整 REST API，Docker 部署友好。

**bigcapitalhq/bigcapital**
面向中小企业的开源财务会计系统，复式记账，支持发票、账单、费用、存货，含智能财务报表，Node.js + React + PostgreSQL，是 QuickBooks/Xero/Wave 的自托管开源替代。

---

## 十七、财富/投资组合追踪 / Wealth & Portfolio Tracking

| 项目 | Stars | 语言 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | --- | --- | -------- |
| **Ghostfolio** | ~8.6k | TypeScript | 需数据 | ★★ | https://github.com/ghostfolio/ghostfolio |
| **Wealthfolio** | ~7.7k | Rust/Tauri | 需数据 | ★★ | https://github.com/wealthfolio/wealthfolio |
| **rotki** | ~3.9k | Python/Vue | 需数据 | — | https://github.com/rotki/rotki |

**Ghostfolio**
开源财富管理软件（Angular + NestJS + Prisma 全栈），支持股票、ETF、加密货币多资产追踪，可自托管，提供资产配置分析、持仓概览和投资绩效报告，适合注重数据主权的个人投资者。

**Wealthfolio**
本地优先（Local-First）的桌面投资组合追踪器，Rust + Tauri 实现，数据全部存储于本地无需账户注册，支持 CSV 导入、净值追踪、Monte Carlo 模拟和 FIRE 退休计算。

**rotki**
隐私优先的开源投资组合追踪与加密货币税务报告工具。数据加密存储于本地，支持 100+ CEX/DeFi 协议的交易导入，自动计算各国税务报告，是最流行的本地化加密资产税务工具。

---

## 十八、机构量化工具 / Institutional Quant Tools

| 项目 | Stars | 语言 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | --- | --- | -------- |
| **gs-quant (Goldman Sachs)** | ~10.9k | Python | 需数据 | — | https://github.com/goldmansachs/gs-quant |

**gs-quant**
高盛开源的 Python 量化金融工具包，构建于其内部风险转移平台之上。提供衍生品定价、结构化产品分析、期权希腊字母计算和风险管理接口，可对接高盛 Marquee Developer API，是罕见的顶级投行开源量化工具。

---

## 十九、金融数据库 / Financial Databases

| 项目 | Stars | 语言 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | --- | --- | -------- |
| **FinanceDatabase** | ~6.7k | Python | 自包含 | ★★ | https://github.com/JerBouma/FinanceDatabase |
| **FinanceToolkit** | ~3.5k | Python | 需数据 | ★★ | https://github.com/JerBouma/FinanceToolkit |
| **arctic (Man Group)** | ~3.1k | Python | 自包含 | — | https://github.com/man-group/arctic |
| **ArcticDB (Man Group)** | ~2.2k | Python/C++ | 自包含 | — | https://github.com/man-group/ArcticDB |

**FinanceDatabase**
包含 30 万+ 证券代码的开源金融数据库，覆盖股票、ETF、基金、指数、货币、加密货币，支持按国家、行业、市值等维度筛选，是构建量化策略选股池和批量数据拉取的实用基础数据源。

**FinanceToolkit**
透明高效的 Python 财务分析库，实现 200+ 财务比率（PE/PB/ROE/Sharpe 等）计算，完整公开所有计算方法避免黑盒依赖，支持股票、ETF、加密等多资产类别，与 FinanceDatabase 同一作者。

**arctic (Man Group)** ⚠️ 维护模式
Man Group 开源的高性能时序/Tick 数据存储库，基于 MongoDB，支持版本控制（类似 git），专为金融 OHLCV 和 Tick 数据设计。现已进入维护模式，官方推荐迁移至 ArcticDB。

**ArcticDB (Man Group)**
Man Group 与 Bloomberg 联合开发的高性能 DataFrame 数据库。Python-native API，C++ 压缩存储引擎，可将 Pandas DataFrame 直接读写到 S3/LMDB，为金融大规模历史数据分析场景优化设计。

---

## 二十、Java 生态 / Java Ecosystem

| 项目 | Stars | 语言 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | --- | --- | -------- |
| **ta4j** | ~2.4k | Java | 自包含 | ★ | https://github.com/ta4j/ta4j |

**ta4j**
Java 生态中最完整的技术分析库。提供可组合策略 API、30+ 分析准则（Sharpe/最大回撤/Calmar 等）、K 线形态识别和内置回测引擎，原生多线程支持可并行回测数百策略，是 Java/Kotlin 量化开发的首选工具。

---

## 二十一、个人财务与会计 / Personal Finance & Accounting

| 项目 | Stars | 语言 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | --- | --- | -------- |
| **Ledger-CLI** | ~5.9k | C++ | 自包含 | — | https://github.com/ledger/ledger |
| **hledger** | ~4.5k | Haskell | 自包含 | — | https://github.com/simonmichael/hledger |
| **beancount** | ~3k | Python | 自包含 | — | https://github.com/beancount/beancount |
| **paisa** | ~3.1k | Go | 自包含 | — | https://github.com/ananthakumaran/paisa |
| **fava** | ~2.4k | Python | 自包含 | — | https://github.com/beancount/fava |

**Ledger-CLI**
纯文本复式记账（Plain Text Accounting）系统的鼻祖，始于 2003 年。所有数据以纯文本文件存储，无数据库依赖，支持多币种和精确财务报告，是 PTA 生态的基础。

**hledger**
Ledger-CLI 的 Haskell 重写版，更快速健壮，提供 CLI、TUI（终端图形界面）和 Web 三种交互方式，是纯文本记账（Plain Text Accounting）生态中目前最活跃的实现。

**beancount**
以严格数据完整性著称的复式记账工具，V3 版本用 C++ 核心显著提升性能。数据格式比 Ledger 更结构化，适合有复杂财务结构（多账户/多币种/股票持仓）的用户。

**paisa**
基于 Ledger/hledger 构建的开源个人财务管理器（Go 实现），专注印度市场（共同基金、NPS、NSE 股票）的净值追踪，数据全部存储于本地，隐私友好。

**fava**
Beancount 的官方 Web UI，提供图表、账户余额、交易流水、收支报告等可视化界面，使 Beancount 的数据分析体验大幅提升，是 Beancount 用户的必装配套工具。

---

## 二十二、Go 语言生态 / Go Ecosystem

| 项目 | Stars | 语言 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | --- | --- | -------- |
| **ticker** | ~6k | Go | 需数据 | ★★ | https://github.com/achannarasappa/ticker |
| **GoCryptoTrader** | ~2.2k | Go | 需数据 | ★ | https://github.com/thrasher-corp/gocryptotrader |
| **mop** | ~2.2k | Go | 需数据 | ★ | https://github.com/mop-tracker/mop |

**ticker**
终端实时行情追踪工具（Go），支持股票、加密货币价格及持仓盈亏实时显示，YAML 配置自选股和成本基础，数据源支持 Yahoo Finance/Coinbase，是开发者在终端环境中查看行情的利器。

**GoCryptoTrader**
Go 语言实现的加密货币交易机器人框架，支持 30+ 主流交易所（Binance、OKX、Bybit 等），提供完整的回测、模拟交易和实盘交易能力，代码质量高，适合 Go 技术栈团队。

**mop**
专为开发者打造的终端股票行情追踪器（Go），界面简洁，实时显示多只股票价格和涨跌，适合在服务器/终端环境快速查看市场数据。

---

## 二十三、AI 每日分析与自动化报告 / AI Daily Analysis

> 代表新兴品类：用 LLM 自动生成每日行情分析报告并多渠道推送，区别于传统量化回测框架。

| 项目 | Stars | 语言 | 市场 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | ------ | --- | --- | -------- |
| **Kronos** | ~27.8k | Python | 🌍 全球 | 需数据 | ★★★ | https://github.com/shiyu-coder/Kronos |
| **AI-Trader (HKUDS)** | ~19.9k | Python | 🌍 全球 | 需数据 | ★★★ | https://github.com/HKUDS/AI-Trader |
| **Vibe-Trading (HKUDS)** | ~13k | Python | 🌍 全球/A股 | 需数据 | ★★★ | https://github.com/HKUDS/Vibe-Trading |
| **daily_stock_analysis** | ~49.6k | Python/TS | 🌍 A/H/US/JP/KR | 需数据 | ★★★ | https://github.com/ZhuLinsen/daily_stock_analysis |
| **TradingAgents-CN** | ~29k | Python | 🇨🇳+ A/H/US | 需数据 | ★★★ | https://github.com/hsliuping/TradingAgents-CN |
| **Stock-Prediction-Models** | ~9.4k | Python | 🌍 全球 | 需数据 | — | https://github.com/huseinzol05/Stock-Prediction-Models |

**Kronos**
首个专为金融 K 线语言设计的开源基础模型（AAAI 2026 收录），预训练于 45+ 全球交易所的 120 亿根 K 线记录，将 OHLCV 量化为 token 后自回归预测价格走势，支持股票/期货/加密多资产，是"K 线的 GPT"，2026 年爆发增长至 27.8k Stars。

**AI-Trader (HKUDS)**
香港大学数据科学组出品的 100% 全自动 Agent 原生交易平台，支持 OpenClaw/Claude Code/Codex/Cursor 等主流 AI Agent 接入，内置集体智能协同交易和一键跟单，覆盖 A 股/港股/美股/加密，2026 年爆发增长。

**Vibe-Trading (HKUDS)**
面向量化研究的 AI 多 Agent 工作台，自然语言驱动，内置 79 个金融技能、452 个预构建 Alpha 因子（Alpha101/GTJA191/qlib158）、6 种回测引擎，覆盖 A 股/全球股票/加密/期货/期权，同一团队出品。

**daily_stock_analysis**
LLM 驱动的多市场每日智能分析系统，2026-06 登上 GitHub Trending 榜 #2 后爆红（49.6k Stars）。自动生成决策看板（核心结论、评分、趋势、买卖点位、风险警报），支持企微/飞书/Telegram/Discord/Slack/Email 多渠道推送，GitHub Actions 定时零成本运行，兼容 OpenAI/Claude/Gemini/DeepSeek/Ollama 等主流模型，覆盖 A 股/港股/美股/日股/韩股。

**TradingAgents-CN**
TradingAgents 的中文增强版，FastAPI + Vue3 全栈，深度适配中国市场（A 股、港股、美股），内置多 LLM 接入（GPT/Claude/DeepSeek/Qwen），支持批量分析、Word/PDF 研报导出，定位学习研究与本地化部署平台。

**Stock-Prediction-Models** ⚠️ 已归档
汇集 18 种深度学习模型（LSTM/GRU/Attention/Transformer 等）和 23 种强化学习交易 Agent 的股价预测代码集合，覆盖主流 ML 技术在金融预测中的实践。2023 年已 archived，仍具较高参考价值。

---

## 二十四、FIX 协议与撮合引擎 / FIX Protocol & Matching Engines

| 项目 | Stars | 语言 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | --- | --- | -------- |
| **QuickFIX** | ~2.5k | C++ | 自包含 | — | https://github.com/quickfix/quickfix |
| **quickfixgo/quickfix** | ~2k | Go | 自包含 | — | https://github.com/quickfixgo/quickfix |
| **exchange-core** | ~2.5k | Java | 自包含 | — | https://github.com/exchange-core/exchange-core |

**QuickFIX**
业界最广泛使用的开源 FIX 协议引擎（C++），是机构级电子交易系统对接券商/交易所的标准基础设施。支持 FIX 4.0–5.0 全版本，有 Java（QuickFIX/J）、Go、.NET 等多语言移植版本。

**quickfixgo/quickfix**
QuickFIX 的 Go 语言完整实现，覆盖 FIX 4.0–5.0SP2 全版本，含 FIX 消息生成工具和完整验收测试套件，是 Go 技术栈接入券商/交易所 FIX 通道的首选。

**exchange-core**
基于 LMAX Disruptor + Eclipse Collections + OpenHFT 的超高速 Java 撮合引擎，在普通硬件上可达 500 万+ ops/秒，支持限价单/市价单/止损单，内置风控和持仓管理，适合构建交易所基础设施。

---

## 二十五、加密货币数据 Feed / Crypto Data Feeds

| 项目 | Stars | 语言 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | --- | --- | -------- |
| **python-binance** | ~7k | Python | 需数据 | ★ | https://github.com/sammchardy/python-binance |
| **cryptofeed** | ~2.7k | Python | 需数据 | ★ | https://github.com/bmoscon/cryptofeed |
| **binance-connector-python** | ~2.6k | Python | 需数据 | ★ | https://github.com/binance/binance-connector-python |

**python-binance**
最流行的非官方 Binance Exchange Python API 封装，支持现货/合约/期权全量 REST 和 WebSocket 接口，内置异步支持，是加密量化交易社区使用最广的 Binance SDK。

**cryptofeed**
统一的加密货币交易所 WebSocket 数据订阅库，支持 Binance/Coinbase/Kraken 等主流交易所，标准化处理 trades/orderbook/ticker/funding rate 等数据流，后端可直写 Redis/Arctic/Kafka/InfluxDB/PostgreSQL 等存储，是构建实时加密数据管道的首选工具。

**binance-connector-python**
Binance 官方维护的轻量级 Python SDK，覆盖现货/合约/杠杆全量 API，代码简洁、文档完整，是 python-binance 的官方替代选择。

---

## 二十六、DeFi 与链上开发 / DeFi & On-chain

| 项目 | Stars | 语言 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | --- | --- | -------- |
| **ethereum-etl** | ~3.1k | Python | 需数据 | — | https://github.com/blockchain-etl/ethereum-etl |
| **ethers.js** | ~8.7k | JavaScript | 需数据 | — | https://github.com/ethers-io/ethers.js |
| **foundry** | ~10k | Rust | 自包含 | — | https://github.com/foundry-rs/foundry |
| **hardhat** | ~7.6k | TypeScript | 自包含 | — | https://github.com/NomicFoundation/hardhat |
| **wagmi** | ~6.7k | TypeScript | 需数据 | — | https://github.com/wevm/wagmi |
| **uniswap/v3-core** | ~5k | Solidity | 自包含 | — | https://github.com/Uniswap/v3-core |
| **viem** | ~3.4k | TypeScript | 需数据 | — | https://github.com/wevm/viem |
| **graph-node** | ~3.1k | Rust | 需数据 | — | https://github.com/graphprotocol/graph-node |
| **web3.py** | ~5.5k | Python | 需数据 | — | https://github.com/ethereum/web3.py |
| **zenbot** | ~8.3k | Node.js | 需数据 | — | https://github.com/DeviaVir/zenbot |
| **brownie** | ~2.5k | Python | 自包含 | — | https://github.com/eth-brownie/brownie |

**ethereum-etl**
将 Ethereum 区块链数据（区块/交易/ERC20 Token/智能合约事件）ETL 到 CSV 或 Google BigQuery 的 Python 工具，是链上数据工程的事实标准，支持流式增量更新，广泛用于 DeFi 数据分析和链上行为研究。

**ethers.js**
完整的 Ethereum JavaScript 库 + 钱包实现，是 DeFi 前端/脚本与区块链交互的事实标准，提供合约 ABI 解码、Provider 抽象、签名工具等，比 web3.js 更轻量安全。

**foundry**
Rust 实现的超快 Ethereum 智能合约开发工具包（Forge 测试 + Cast CLI + Anvil 本地节点），编译和测试速度远超 Hardhat，已成为 DeFi 合约开发的首选工具链。

**hardhat**
Ethereum 智能合约编译、部署、测试、调试一体化开发环境，TypeScript 生态，插件体系丰富，曾是 DeFi 协议开发的长期标准工具（现被 Foundry 追赶）。

**wagmi**
React Hooks for Ethereum，封装钱包连接、合约读写、签名等操作，是 Web3 前端开发的标准库，与 viem 深度集成，支持所有主流 EVM 链。

**uniswap/v3-core**
Uniswap V3 核心智能合约，实现集中流动性 AMM（Automated Market Maker），是链上去中心化交易所的基础协议参考实现，DeFi 领域影响最深远的开源合约之一。

**viem**
轻量级高性能 TypeScript Ethereum 接口库，比 ethers.js 更现代，Tree-shakeable 设计，与 wagmi v2 深度集成，适合对 bundle size 敏感的 Web3 前端项目。

**graph-node**
The Graph 协议的区块链数据索引节点（Rust 实现），将 Ethereum 链上事件通过 Subgraph 定义索引后提供 GraphQL 查询，是 DeFi 数据基础设施的核心组件，支持多链。

**web3.py**
以太坊官方 Python 接口库，用于与以太坊区块链及 DeFi 协议交互，支持合约调用、事件监听、交易发送、ENS 解析等，是 Python 开发者构建 DeFi 应用和链上数据分析的基础工具。

**zenbot** ⚠️ 已归档
命令行加密货币 TA 交易机器人（Node.js + MongoDB），支持 Binance/Kraken 等主流交易所，2020 年停止维护。Stars 约 8.3k，历史影响力较高，现已不建议生产使用。

**brownie** ⚠️ 已归档
基于 Python 的以太坊智能合约开发与测试框架，曾是 DeFi 开发的标准工具之一（Yearn Finance、Curve 等项目使用），现已停止维护，官方建议迁移至 Ape Framework。

---

## 二十七、宏观经济数据 / Macro & Economic Data

| 项目 | Stars | 语言 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | --- | --- | -------- |
| **pandas-datareader** | ~3.2k | Python | 需数据 | ★ | https://github.com/pydata/pandas-datareader |
| **QuantEcon.py** | ~2.4k | Python | 自包含 | — | https://github.com/QuantEcon/QuantEcon.py |

**pandas-datareader**
PyData 维护的多数据源 Python 读取库，支持 FRED（美联储经济数据库）、World Bank、Fama-French 三因子、OECD、Eurostat 等宏观经济数据源，返回标准 Pandas DataFrame，是量化研究中获取宏观数据的标准工具。

**QuantEcon.py**
诺贝尔经济学奖得主 Thomas Sargent 团队出品的量化经济学 Python 库，覆盖马尔可夫链、动态规划、线性代数、博弈论等计算经济学核心模型，配套完整讲义（quantecon.org），是学习计算经济学的权威开源工具。

---

## 二十八、券商 API 接口（补充）/ Broker APIs

> 补充至第十五节"券商 API 接口"，此处列出星数接近门槛的参考项目。

| 项目 | Stars | 语言 | 市场 | 三方接口 | 值得联系 | GitHub | 备注 |
| ------ | ------- | ------ | ------ | -------- | --- | --- | ------ |
| **alpaca-trade-api** | ~1.8k | Python | 🌍 美股 | https://github.com/alpacahq/alpaca-trade-api-python | 接近门槛 | 需数据 | — |

**alpaca-trade-api**
Alpaca 券商（佣金免费美股交易）的官方 Python SDK，提供历史数据、实时行情、下单/撤单等接口，是美国量化散户常用的免费实盘接口，与 zipline/backtrader 等框架集成良好。Stars 约 1.8k，接近收录门槛。

---

## 二十九、时序数据库 / Time Series Databases

| 项目 | Stars | 语言 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | --- | --- | -------- |
| **timescaledb** | ~23k | C/SQL | 自包含 | ★ | https://github.com/timescale/timescaledb |
| **questdb** | ~15k | Java | 自包含 | ★ | https://github.com/questdb/questdb |

**timescaledb**
基于 PostgreSQL 的高性能时序 SQL 数据库扩展，原生支持时序数据压缩、连续聚合和数据保留策略，与现有 PostgreSQL 生态完全兼容，是存储金融 tick 数据和行情历史的首选之一。

**questdb**
专为高性能实时分析设计的开源时序数据库，列式存储 + SIMD 向量化加速，内置 InfluxDB Line Protocol 和 PostgreSQL wire protocol，内置金融 tick 数据示例，DB-Engines 中增速最快的时序数据库之一。

---

## 三十、Rust 生态 / Rust Ecosystem

| 项目 | Stars | 语言 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | --- | --- | -------- |
| **barter-rs** | ~2.2k | Rust | 需数据 | ★★ | https://github.com/barter-rs/barter-rs |

**barter-rs**
事件驱动的 Rust 算法交易生态系统，基于 Tokio 异步架构，强类型安全，支持实盘/模拟/回测三种运行模式，是目前最完整的 Rust 量化框架。适合对延迟和内存安全有极致要求的量化团队。

---

## 快速选型参考

| 需求场景 | 推荐项目 |
| --------- | --- | --- | --------- |
| 投资研究平台（Bloomberg 替代） | OpenBB / FinceptTerminal |
| A 股量化交易（国内接口） | vnpy / rqalpha 🇨🇳 / WonderTrader 🇨🇳 |
| 策略回测（Python 生态） | backtrader / backtesting.py / vectorbt |
| 快速策略原型验证 | backtesting.py |
| 大规模参数寻优 | vectorbt |
| 系统化期货交易 | pysystemtrade |
| 生产级高性能交易 | nautilus_trader / Lean |
| 高频/做市策略回测 | hftbacktest |
| 加密货币自动交易 | freqtrade / CCXT + jesse |
| 加密做市/套利 | Hummingbot |
| 加密资产税务报告 | rotki |
| 加密数据 Feed | cryptofeed |
| DeFi/链上开发 | web3.py |
| A 股数据获取 | AKShare / tushare 🇨🇳 |
| 全球股票数据 | yfinance |
| 宏观/FRED 经济数据 | pandas-datareader |
| 30 万+ 证券目录 | FinanceDatabase |
| 金融时序大数据存储 | ArcticDB |
| 技术指标计算 | TA-Lib / pandas-ta |
| Java 技术分析 | ta4j |
| Alpha 因子研究 | alphalens |
| 投资组合优化 | PyPortfolioOpt / Riskfolio-Lib |
| 策略绩效分析 | pyfolio / QuantStats |
| 期权/衍生品定价 | QuantLib / gs-quant |
| FIX 协议对接 | QuickFIX |
| 交易所撮合引擎 | exchange-core |
| AI/ML 量化研究 | Qlib / FinRL |
| LLM 多智能体交易 | TradingAgents / ai-hedge-fund |
| AI 每日行情分析+推送 | daily_stock_analysis |
| A 股 AI 量化一体化 | Qbot 🇨🇳 / TradingAgents-CN 🇨🇳 |
| 金融大模型微调 | FinGPT |
| ML 量化研究（书配套） | mlfinlab |
| 贝叶斯金融建模 | PyMC / Bayesian Methods for Hackers |
| 计量经济学/时序建模 | statsmodels |
| SEC 财报/内部人交易解析 | edgartools |
| 加密货币可视化策略 | Superalgos |
| DeFi 前端开发 | ethers.js / wagmi / viem |
| Ethereum 合约开发 | foundry / hardhat |
| 链上数据索引 | graph-node |
| 个人财务全功能平台 | maybe-finance |
| 开源企业会计（ERP） | erpnext / bigcapital |
| 个人预算（YNAB 替代） | Actual Budget |
| 金融 NLP/情绪分析 | FinBERT |
| IB 实盘接口 | ib_insync / ib_async |
| Robinhood 接口 | robin_stocks |
| 美股免费实盘接口 | alpaca-trade-api |
| Web 交易图表 | lightweight-charts |
| React K 线图 | react-stockcharts |
| Python K 线图 | mplfinance |
| 终端行情追踪 | ticker（Go）|
| Go 加密货币交易 | GoCryptoTrader |
| 个人投资组合追踪 | Ghostfolio / Wealthfolio |
| 个人/家庭记账 | Firefly III / beancount + fava |
| 纯文本记账 | Ledger-CLI / hledger |
| 时序数据库（tick/行情存储） | timescaledb / questdb / ArcticDB |
| 金融大数据处理（替代 pandas） | Polars |
| HFT 消息总线 | LMAX Disruptor / Aeron |
| 交易系统事件持久化 | Chronicle-Queue |
| Rust 量化交易 | barter-rs |
| 开源支付网关/编排 | hyperswitch |
| 核心银行/微金融平台 | apache/fineract |
| 财经新闻/社区情绪分析 | vaderSentiment |
| Go FIX 协议引擎 | quickfixgo/quickfix |
| HFT 消息编解码（SBE） | simple-binary-encoding |
| HFT 堆外 K/V 缓存 | Chronicle-Map |
| Binance API | python-binance / binance-connector-python |
| 链上数据 ETL | ethereum-etl |
| 期权/衍生品 GPU 批量定价 | tf-quant-finance |
| Web 可视化大盘/监控 | Grafana |
| Python 交互式 Dashboard | Plotly Dash / Panel |
| 国内前端 K 线图 | KLineChart / ECharts / pyecharts |
| 金融 K 线基础模型 | Kronos |
| 全自动 AI Agent 交易 | AI-Trader / Vibe-Trading |
| RL 强化学习交易 | stable-baselines3 + gym-anytrading + FinRL |
| 开源 BI/数据看板 | Apache Superset / Metabase / Redash |
| 流式金融数据表格 | Perspective (FINOS) / AG Grid / TanStack Table |
| React 图表库 | Recharts / ApexCharts / nivo |
| 基础可视化底层 | D3.js |
| 企业 ERP/会计 | odoo / akaunting / Dolibarr / erpnext |
| 加密资产数据目录 | best-of-crypto |
| 加密交易机器人列表 | awesome-crypto-trading-bots |
| 全球股票免费 API | alpha_vantage / yfinance |
| A 股免费数据 | AKShare / adata / efinance / tushare 🇨🇳 |

---

## 三十一、ML 量化研究 / ML Quant Research

| 项目 | Stars | 语言 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | --- | --- | -------- |
| **mlfinlab** | ~4.4k | Python | 需数据 | ★★ | https://github.com/hudson-and-thames/mlfinlab |

**mlfinlab**
Hudson & Thames 出品，基于 Marcos Lopez de Prado《金融机器学习》系列著作的实现库，提供三重屏障标注、元标注、分数差分、特征工程等核心算法的可复现实现。注意：部分高级功能已转为订阅制，开源版仍覆盖书中核心内容。

---

## 三十二、统计与计量经济学 / Statistics & Econometrics

| 项目 | Stars | 语言 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | --- | --- | -------- |
| **Bayesian Methods for Hackers** | ~28k | Python | 自包含 | — | https://github.com/CamDavidsonPilon/Probabilistic-Programming-and-Bayesian-Methods-for-Hackers |
| **statsmodels** | ~11.4k | Python | 自包含 | — | https://github.com/statsmodels/statsmodels |
| **PyMC** | ~9.6k | Python | 自包含 | — | https://github.com/pymc-devs/pymc |

**Bayesian Methods for Hackers**
贝叶斯方法与概率编程的开源教学书（Python 实现），以计算为第一视角而非数学推导，配套 PyMC 代码示例，28k Stars，是学习贝叶斯金融统计建模的最佳入门资源，适合无深厚统计背景的量化研究者。

**statsmodels**
Python 计量经济学与统计建模标准库，提供回归分析（OLS/GLS/WLS）、时序模型（ARIMA/VAR/GARCH）、假设检验、生存分析等，是量化金融研究和学术计量分析的基础工具。

**PyMC**
Python 贝叶斯统计建模与概率编程框架，支持 MCMC 采样（NUTS/HMC）和变分推断，广泛用于金融风险建模、参数不确定性量化、因子模型的贝叶斯估计和收益分布拟合。

---

## 三十三、SEC 文件解析 / SEC Filing Tools

| 项目 | Stars | 语言 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | --- | --- | -------- |
| **edgartools** | ~2.5k | Python | 需数据 | ★ | https://github.com/dgunning/edgartools |

**edgartools**
Python SEC EDGAR 文件解析库，支持 10-K/10-Q/8-K/13F/Form 4/DEF 14A 等所有主要文件类型，清洁 API 直接返回结构化 Pandas DataFrame，是解析美股财报、内部人交易和机构持仓数据的最佳开源工具。

---

## 三十四、高性能交易基础设施 / HFT Infrastructure

> 专为低延迟交易系统、撮合引擎和金融消息总线设计的基础组件库。

| 项目 | Stars | 语言 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | --- | --- | -------- |
| **Polars** | ~35.6k | Rust/Python | 自包含 | ★ | https://github.com/pola-rs/polars |
| **LMAX Disruptor** | ~18.2k | Java | 自包含 | — | https://github.com/LMAX-Exchange/disruptor |
| **Aeron** | ~8.7k | Java/C++ | 自包含 | — | https://github.com/aeron-io/aeron |
| **simple-binary-encoding** | ~3.4k | Java/C++ | 自包含 | — | https://github.com/real-logic/simple-binary-encoding |
| **Chronicle-Map** | ~2.9k | Java | 自包含 | — | https://github.com/OpenHFT/Chronicle-Map |
| **Chronicle-Queue** | ~3.5k | Java | 自包含 | — | https://github.com/OpenHFT/Chronicle-Queue |

**Polars**
Rust 实现的极速 DataFrame 引擎，多线程 + SIMD 向量化加速，量化金融领域正快速取代 pandas 成为数据处理标准工具，处理大规模历史行情数据的速度比 pandas 快 10~100 倍，支持 Python/R/Node.js 接口。

**LMAX Disruptor**
LMAX 交易所开源的超高性能线程间消息传递库，使用无锁环形缓冲区（Ring Buffer），是 exchange-core 等高性能撮合引擎的核心依赖，延迟比 ArrayBlockingQueue 低 3 个数量级，是金融系统内部高吞吐通信的事实标准。

**Aeron**
Real Logic 出品的超低延迟 UDP 消息传输库，支持进程间通信（IPC）/单播/多播，在物理硬件上单程延迟约 18 微秒，是全球资本市场高频交易系统的传输层标准之一，Chronicle 和 LMAX 生态的重要组件。

**simple-binary-encoding (SBE)**
Real Logic（Aeron 同一团队）出品的 FIX SBE 标准参考实现，针对低延迟金融消息设计的二进制编解码规范，支持 Java/C/C++/C#/Go/Rust 多语言，是交易所和高频交易系统消息序列化的行业标准，延迟比 Protobuf 低一个数量级。

**Chronicle-Map**
OpenHFT 出品的超快堆外共享内存 K/V 存储，数据存于内存映射文件而非 JVM 堆，支持跨进程/跨服务器访问，大小仅受磁盘容量限制，在生产级实时交易系统（参考数据、持仓缓存）中广泛使用。

**Chronicle-Queue**
OpenHFT 出品的微秒级持久化消息队列，数据写入内存映射文件实现零拷贝持久化，专为交易系统的事件溯源和完整交易记录设计，支持百万级 TPS，可与 Chronicle-Map 配套构建完整交易系统存储层。

---

## 三十五、支付与核心银行基础设施 / Payments & Core Banking

| 项目 | Stars | 语言 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | --- | --- | -------- |
| **hyperswitch** | ~42k | Rust | 需数据 | — | https://github.com/juspay/hyperswitch |
| **apache/fineract** | ~2.1k | Java | 自包含 | — | https://github.com/apache/fineract |

**hyperswitch**
Juspay 出品、Rust 实现的开源可组合支付编排平台，支持 200+ 支付处理器（Stripe/PayPal/Adyen 等）和 150+ 支付方式，PCI DSS 合规，可完全自托管，是目前 Stars 最高的开源支付网关/路由基础设施，适合构建自主可控的支付系统。

**apache/fineract**
Apache 基金会维护的开源微金融/核心银行平台，被全球 400+ 金融机构使用，覆盖贷款管理、储蓄账户、应收账款、会计总账，是开发中国家普惠金融和数字银行基础设施的主力开源平台。

---

## 三十六、金融情绪分析 / Financial Sentiment

| 项目 | Stars | 语言 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | --- | --- | -------- |
| **vaderSentiment** | ~4.9k | Python | 自包含 | — | https://github.com/cjhutto/vaderSentiment |
| **pytrends** | ~3.6k | Python | 需数据 | — | https://github.com/GeneralMills/pytrends |

**vaderSentiment**
专为社交媒体短文本设计的规则+词典情感分析工具，无需训练数据即可使用，广泛用于财经新闻/股票社区/电话会纪要的情绪打分，可直接作为量化因子输入，是金融 NLP 中最常用的基线情绪工具。

**pytrends** ⚠️ 已归档
Google Trends 非官方 Python 接口，用于获取关键词搜索热度时序数据，可作为另类投资信号（市场关注度、投资者情绪代理变量）。2025 年 4 月已归档停止维护，可能随 Google API 变更而失效。

---

## 三十七、企业财务与 ERP / Enterprise Finance & ERP

| 项目 | Stars | 语言 | 三方接口 | 值得联系 | GitHub |
| ------ | ------- | ------ | --- | --- | -------- |
| **odoo/odoo** | ~49.1k | Python/JS | 自包含 | — | https://github.com/odoo/odoo |
| **akaunting/akaunting** | ~9.9k | PHP | 自包含 | — | https://github.com/akaunting/akaunting |
| **Dolibarr/dolibarr** | ~7.2k | PHP | 自包含 | — | https://github.com/Dolibarr/dolibarr |

**odoo/odoo**
世界最流行的开源 ERP 系统（12M+ 用户），内置完整会计模块（总账/应收应付/多币种/财务报表），同时覆盖 CRM、电商、HR、制造，是 SAP/QuickBooks 的强力开源替代，模块化设计可按需启用。

**akaunting/akaunting**
面向小企业和自由职业者的开源在线会计软件，支持发票、账单、费用追踪、多币种报表，Laravel + VueJS 实现，App Store 插件生态丰富，是 Wave/Xero 的自托管替代选择。

**Dolibarr/dolibarr**
功能全面的开源 ERP+CRM，覆盖联系人管理、供应商、发票、订单、库存、会计账本，适合中小企业和非营利组织，GPL-3 许可，百万级用户，支持多语言多货币。

---

## 附录：实用但未达 2000 Stars 的参考项目

> 以下项目在各自领域有实际使用价值，但 Stars 未达收录门槛，供参考。

| 项目 | Stars | 语言 | 市场 | 三方接口 | 值得联系 | GitHub | 说明 |
| ------ | ------- | ------ | ------ | -------- | --- | --- | ------ |
| **empyrical** | ~1.4k | Python | 🌍 全球 | https://github.com/quantopian/empyrical | Quantopian 出品的金融风险指标库，Sharpe/Sortino/最大回撤等，pyfolio/zipline 的底层依赖 |  |  |
| **alpaca-trade-api** | ~1.8k | Python | 🌍 美股 | https://github.com/alpacahq/alpaca-trade-api-python | Alpaca 券商官方 Python SDK，免佣金美股实盘接口，与回测框架集成良好 | 需数据 | — |
| **QuickFIX/J** | ~1.1k | Java | 🌍 全球 | https://github.com/quickfix-j/quickfixj | QuickFIX 的 Java 实现，机构级 FIX 协议引擎，Java 技术栈首选 | 自包含 | — |
| **FinNLP** | ~1.3k | Python | 🌍 全球 | https://github.com/AI4Finance-Foundation/FinNLP | AI4Finance 出品的金融 NLP 数据工具，提供新闻/社媒/研报的标准化抓取和标注 |  |  |
| **pytdx** | ~1k | Python | 🇨🇳 仅A股 | https://github.com/rainx/pytdx | 通达信 Python 接口，可获取 A 股实时行情和历史数据，适合无需注册即可获取数据的场景 |  |  |
| **baostock** | ~1k | Python | 🇨🇳 仅A股 | https://github.com/BaoStock/baostock | 免费开源 A 股历史数据接口，覆盖日/周/月 K 线、财务报表、分红送股等 |  |  |
| **easyquotation** | ~900 | Python | 🇨🇳 仅A股 | https://github.com/shidenggui/easyquotation | 免费获取 A 股实时行情，支持新浪/腾讯数据源，响应速度快，适合实时监控场景 | 自包含 | ★ |
| **ta-rs** | ~800 | Rust | 🌍 全球 | https://github.com/greyblake/ta-rs | Rust 实现的技术分析指标库（MA/RSI/MACD 等），无运行时依赖，适合嵌入 Rust 交易系统 | 自包含 | ★ |
| **rateslib** | ~350 | Python | 🌍 全球 | https://github.com/attack68/rateslib | 固定收益定价库，支持利率互换/债券/期权定价，与 QuantLib 互补，API 更 Pythonic |  |  |
| **skfolio** | ~1.8k | Python | 🌍 全球 | https://github.com/skfolio/skfolio | scikit-learn 兼容的投资组合优化库，支持均值方差/HRP/CVaR 等，与 sklearn Pipeline 无缝集成 | 可选 | ★★ |
| **marketstore** | ~1.9k | Go | 🌍 全球 | https://github.com/alpacahq/marketstore | Alpaca 出品的 Go 语言金融时序数据库，专为 OHLCV 数据设计，支持 gRPC 查询 |  |  |
| **zipline-reloaded** | ~1.7k | Python | 🌍 全球 | https://github.com/stefan-jansen/zipline-reloaded | Stefan Jansen 维护的 zipline Python 3.x 兼容 fork，《机器学习与算法交易》配套使用 | 需数据 | ★ |
| **invoiceninja** | ~9.8k | PHP | 🌍 全球 | https://github.com/invoiceninja/invoiceninja | 开源发票/报价/时间追踪系统，适合自由职业者和小企业管理应收账款 |  |  |
| **spliit** | ~2.7k | TypeScript | 🌍 全球 | https://github.com/spliit-app/spliit | Splitwise 的免费开源替代，Next.js 实现，支持多人分账和旅行账单，可完全自托管 |  |  |
| **react-financial-charts** | ~1.4k | TypeScript | 🌍 全球 | https://github.com/reactivemarkets/react-financial-charts | react-stockcharts 的 TypeScript 社区继任者，持续维护，支持 React 18 |  |  |
| **cvxportfolio** | ~1.2k | Python | 🌍 全球 | https://github.com/cvxgrp/cvxportfolio | Stanford Boyd 组 + BlackRock 联合出品，基于 CVXPY 的多期组合优化与回测框架 |  |  |
| **pyfolio-reloaded** | ~500 | Python | 🌍 全球 | https://github.com/stefan-jansen/pyfolio-reloaded | Stefan Jansen 维护的 pyfolio 现代化 fork，Python 3.x 兼容，与 zipline-reloaded 配套 | 需数据 | ★★ |
| **alphalens-reloaded** | ~438 | Python | 🌍 全球 | https://github.com/stefan-jansen/alphalens-reloaded | Stefan Jansen 维护的 alphalens fork，修复原版遗留 bug，持续更新 | 需数据 | ★★ |
| **fredapi** | ~1.6k | Python | 🌍 全球 | https://github.com/mortada/fredapi | 美联储 FRED 经济数据库 Python 封装，支持时序数据和修订历史查询 |  |  |
| **polygon-api-client** | ~1.3k | Python | 🌍 美股 | https://github.com/polygon-io/client-python | Polygon.io 官方 Python 客户端，提供实时/历史美股、期权、外汇数据 |  |  |
| **pycoingecko** | ~1.1k | Python | ⛓️ 加密 | https://github.com/man-iam/pycoingecko | CoinGecko API Python 封装，加密数据领域最常用的免费数据源接口 |  |  |
| **ethereum-etl (bitcoin)** | ~454 | Python | ⛓️ 加密 | https://github.com/blockchain-etl/bitcoin-etl | bitcoin-etl，比特币链 ETL 工具，同 ethereum-etl 同一组织 | 需数据 | — |
| **yhilpisch/py4fi2nd** | ~2k | Python | 🌍 全球 | https://github.com/yhilpisch/py4fi2nd | 《Python for Finance》第2版（Yves Hilpisch 著）配套 Jupyter Notebooks |  |  |
| **EA31337/EA31337** | ~1.1k | MQL | 🌍 外汇 | https://github.com/EA31337/EA31337 | MetaTrader 4/5 平台的开源 EA（外汇自动交易机器人）框架，最活跃的 MT 开源项目 |
| **cryo** | ~1.6k | Rust | ⛓️ 加密 | https://github.com/paradigmxyz/cryo | Paradigm 出品，Rust 实现，最快的以太坊链上数据提取工具，输出 Parquet/CSV，接近门槛 |  |  |
