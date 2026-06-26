# 热门金融/股票/交易终端开源项目汇总

> 收录 GitHub Stars 超过 2000 的热门金融开源项目，按分类整理。
> 数据更新时间：2026-06（持续补充中）

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

---

## 一、交易终端 / Trading Terminals

| 项目 | Stars | 语言 | GitHub |
|------|-------|------|--------|
| **OpenBB** | ~69k | Python | https://github.com/OpenBB-finance/OpenBB |
| **StockSharp** | ~7k | C# | https://github.com/StockSharp/StockSharp |

### 项目简介

**OpenBB**
Bloomberg Terminal 的开源替代品。集成 AI 驱动的投资研究功能，支持股票、ETF、加密货币、宏观经济数据等多数据源，可通过 MCP 或 Python SDK 扩展。是当前最完整的开源投资研究平台。

**StockSharp**
企业级算法交易与量化交易开源平台（.NET 生态）。支持股票、外汇、加密货币和期权，提供图形化策略设计器，适合 C# 技术栈团队。

---

## 二、量化回测框架 / Quant Backtesting Frameworks

| 项目 | Stars | 语言 | GitHub |
|------|-------|------|--------|
| **vnpy / vn.py** | ~25k | Python | https://github.com/vnpy/vnpy |
| **backtrader** | ~22k | Python | https://github.com/mementum/backtrader |
| **zipline** | ~20k | Python | https://github.com/quantopian/zipline |
| **abu（阿布量化）** | ~12k | Python | https://github.com/bbfamily/abu |
| **Lean (QuantConnect)** | ~10k | C#/Python | https://github.com/QuantConnect/Lean |
| **backtesting.py** | ~8k | Python | https://github.com/kernc/backtesting.py |
| **WonderTrader** | ~6.2k | C++/Python | https://github.com/wondertrader/wondertrader |
| **rqalpha** | ~6.4k | Python | https://github.com/ricequant/rqalpha |
| **zvt** | ~4.2k | Python | https://github.com/zvtvz/zvt |
| **vectorbt** | ~4k | Python | https://github.com/polakowo/vectorbt |
| **nautilus_trader** | ~4k | Python/Rust | https://github.com/nautechsystems/nautilus_trader |
| **pysystemtrade** | ~3k | Python | https://github.com/pst-group/pysystemtrade |
| **TradeMaster** | ~2.8k | Python | https://github.com/TradeMaster-NTU/TradeMaster |
| **bt** | ~2.7k | Python | https://github.com/pmorissette/bt |
| **ffn** | ~2.4k | Python | https://github.com/pmorissette/ffn |

### 项目简介

**vnpy / vn.py**
国内最流行的开源量化交易平台。支持 CTP、XTP、Interactive Brokers 等国内外多种交易接口，社区活跃，适合国内量化团队。

**backtrader**
功能完整的 Python 回测框架，事件驱动架构，内置 150+ 技术指标，支持多数据源和实时交易对接。入门友好，文档详尽。

**zipline**
Quantopian 出品的 Pythonic 算法交易库。事件驱动架构，与 pyfolio/alphalens 配合使用，适合学术研究和策略研究。（Quantopian 已关闭，由社区维护）

**WonderTrader**
国产开源量化研发交易一站式框架，C++ 高性能引擎，支持 CTA、HFT、期权等多策略类型，提供完整 Python 接口，适合对性能有要求的国内量化团队。

**rqalpha**
米宽科技开源的可扩展 Python 算法回测和实盘交易框架，支持 A 股、期货、期权等多种证券类型，国内量化社区广泛使用。

**Lean (QuantConnect)**
QuantConnect 云平台背后的开源引擎。支持股票、期权、期货、外汇、加密货币的回测与实盘，多语言接口（C#/Python）。

**zvt**
模块化量化框架，用统一方式处理数据记录、因子计算、证券选择、回测和实时交易，提供实时图表展示，设计理念清晰，二次开发友好。

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

**abu（阿布量化）**
国产开源量化交易平台，支持 A 股、美股、期货、比特币，深度整合机器学习（自动策略优化、交易行为分析），配套《量化交易如何寻找标的》书籍。

**backtesting.py**
基于 Pandas/NumPy/Bokeh 的简洁 Python 回测库，API 极简，支持 TA-Lib、pandas-ta 等指标库，结果以可交互 Bokeh 图表呈现，适合快速验证策略原型。

**TradeMaster**
南洋理工大学出品，专注于强化学习（RL）在量化交易中的应用，内置多种 RL 算法和金融环境。

---

## 三、股票市场数据 / Market Data

| 项目 | Stars | 语言 | GitHub |
|------|-------|------|--------|
| **yfinance** | ~17k | Python | https://github.com/ranaroussi/yfinance |
| **tushare** | ~13k | Python | https://github.com/waditu/tushare |
| **AKShare** | ~11.6k | Python | https://github.com/akfamily/akshare |

### 项目简介

**yfinance**
从 Yahoo Finance 下载历史和实时行情数据的事实标准 Python 库。简单易用，支持股票、ETF、期权链、宏观数据。

**tushare**
国内最早的 A 股数据工具，现已升级为专业数据平台 Tushare Pro。免费版提供基础行情，付费版覆盖更多数据。

**AKShare**
完全免费的中国股票/港股/美股财经数据接口库。聚合新浪、东方财富、雪球等数十个数据源，覆盖面极广。

---

## 四、技术分析 / Technical Analysis

| 项目 | Stars | 语言 | GitHub |
|------|-------|------|--------|
| **TA-Lib (Python)** | ~10k | Python/C | https://github.com/TA-Lib/ta-lib-python |
| **pandas-ta** | ~5k | Python | https://github.com/twopirllc/pandas-ta |
| **ta** | ~4k | Python | https://github.com/bukosabino/ta |

### 项目简介

**TA-Lib (Python)**
业界标准 TA-Lib 的 Python 封装，提供 150+ 技术指标（MA、RSI、MACD、布林带等），底层 C 实现性能极高。

**pandas-ta**
纯 Python 实现的技术分析库，130+ 指标，无需安装 TA-Lib 即可运行。支持 Pandas DataFrame 链式调用，API 简洁。

**ta**
基于 Pandas/NumPy 的轻量级技术分析库，覆盖趋势、动量、波动率、成交量 4 大类指标，代码简洁易读。

---

## 五、算法/自动化交易 / Algorithmic Trading

| 项目 | Stars | 语言 | GitHub |
|------|-------|------|--------|
| **freqtrade** | ~50k | Python | https://github.com/freqtrade/freqtrade |
| **CCXT** | ~42k | JS/Python/PHP | https://github.com/ccxt/ccxt |
| **Hummingbot** | ~19k | Python | https://github.com/hummingbot/hummingbot |
| **Qlib (Microsoft)** | ~18k | Python | https://github.com/microsoft/qlib |
| **FinRL** | ~11.5k | Python | https://github.com/AI4Finance-Foundation/FinRL |
| **jesse** | ~6.5k | Python | https://github.com/jesse-ai/jesse |
| **OctoBot** | ~3.5k | Python | https://github.com/Drakkar-Software/OctoBot |

### 项目简介

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

**OctoBot**
高度可扩展的加密货币交易机器人，插件架构支持自定义策略和技术分析模块。

---

## 六、投资组合与风险管理 / Portfolio & Risk

| 项目 | Stars | 语言 | GitHub |
|------|-------|------|--------|
| **PyPortfolioOpt** | ~5.8k | Python | https://github.com/robertmartin8/PyPortfolioOpt |
| **pyfolio** | ~5k | Python | https://github.com/quantopian/pyfolio |
| **QuantStats** | ~4k | Python | https://github.com/ranaroussi/quantstats |
| **Riskfolio-Lib** | ~3k | Python | https://github.com/dcajasn/Riskfolio-Lib |

### 项目简介

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

| 项目 | Stars | 语言 | GitHub |
|------|-------|------|--------|
| **QuantLib** | ~5k | C++ | https://github.com/lballabio/QuantLib |
| **QuantLib-SWIG** | ~2k | Python/Java/R | https://github.com/lballabio/QuantLib-SWIG |

### 项目简介

**QuantLib**
量化金融领域最权威的开源建模库（C++）。覆盖期权定价（Black-Scholes、Heston 等）、利率模型（Hull-White、LMM）、信用风险、固定收益等。被金融机构广泛用于定价引擎。

**QuantLib-SWIG**
QuantLib 的多语言绑定，提供 Python、Java、R、C# 等语言接口，让非 C++ 用户也能访问 QuantLib 全部功能。

---

## 八、加密货币交易 / Crypto Trading

| 项目 | Stars | 语言 | GitHub |
|------|-------|------|--------|
| **freqtrade** | ~50k | Python | https://github.com/freqtrade/freqtrade |
| **CCXT** | ~42k | JS/Python/PHP | https://github.com/ccxt/ccxt |
| **Hummingbot** | ~19k | Python | https://github.com/hummingbot/hummingbot |
| **jesse** | ~6.5k | Python | https://github.com/jesse-ai/jesse |
| **OctoBot** | ~3.5k | Python | https://github.com/Drakkar-Software/OctoBot |

> 注：以上项目均在"算法交易"分类中有详细介绍，加密货币方向是其主要应用场景。

---

## 九、财务基本面分析 / Fundamental Analysis

| 项目 | Stars | 语言 | GitHub |
|------|-------|------|--------|
| **OpenBB** | ~69k | Python | https://github.com/OpenBB-finance/OpenBB |
| **machine-learning-for-trading** | ~17k | Python | https://github.com/stefan-jansen/machine-learning-for-trading |
| **financepy** | ~2k | Python | https://github.com/domokane/FinancePy |

### 项目简介

**machine-learning-for-trading**
《机器学习与算法交易》（Stefan Jansen 著）配套代码仓库。覆盖因子挖掘、NLP 情绪分析、深度学习预测等 ML 在量化中的完整实践，是学习量化 ML 的最佳资源之一。

**financepy**
纯 Python 实现的金融产品定价库，覆盖固定收益、股权衍生品、外汇、信用产品，代码结构清晰，适合学习金融工程原理。

---

## 十、金融可视化 / Financial Visualization

| 项目 | Stars | 语言 | GitHub |
|------|-------|------|--------|
| **lightweight-charts** | ~15k | TypeScript | https://github.com/tradingview/lightweight-charts |
| **react-stockcharts** | ~4k | JavaScript | https://github.com/rrag/react-stockcharts |
| **mplfinance** | ~4k | Python | https://github.com/matplotlib/mplfinance |
| **finplot** | ~2k | Python | https://github.com/highfellow/finplot |

### 项目简介

**lightweight-charts**
TradingView 出品的高性能前端金融图表库。仅 45KB（gzip），基于 HTML5 Canvas 渲染，支持 K 线、面积图、柱状图，帧率极高，是构建 Web 交易终端的首选图表库。

**mplfinance**
Matplotlib 官方维护的金融数据可视化扩展，原名 `matplotlib.finance`。支持 K 线图、OHLCV 柱状图、均线叠加，与 Pandas DataFrame 无缝集成。

**react-stockcharts**
基于 React + D3 的高度可定制 K 线图组件库，曾是 React 生态中最流行的金融图表库，现已停止维护，react-financial-charts 是其 TypeScript 社区继任者。

**finplot**
专为金融数据设计的高性能 Python 绘图库，基于 PyQtGraph，低延迟实时渲染，适合构建本地桌面交易分析工具。

---

## 十一、精选资源列表 / Curated Lists

| 项目 | Stars | GitHub |
|------|-------|--------|
| **awesome-quant** | ~20k | https://github.com/wilsonfreitas/awesome-quant |
| **awesome-systematic-trading** | ~8.4k | https://github.com/wangzhe3224/awesome-systematic-trading |
| **awesome-ai-in-finance** | ~3.5k | https://github.com/georgezouq/awesome-ai-in-finance |
| **best-of-algorithmic-trading** | ~3k | https://github.com/merovinh/best-of-algorithmic-trading |

### 项目简介

**awesome-quant**
量化金融领域最全面的开源资源汇总列表（Awesome 系列）。按语言（Python/R/Julia/Matlab/C++）分类，涵盖数据、回测、ML、风险管理、研究论文等，是寻找量化工具的第一入口。

**awesome-systematic-trading**
系统化交易精选资源，涵盖加密货币、股票、期货、外汇等资产类别，包含论文、书籍、工具、数据源推荐。

**awesome-ai-in-finance**
精选 LLM 与深度学习在金融市场应用的资源列表，覆盖 AI Agent 交易、策略、数据源、NLP 情绪分析等方向，跟踪金融 AI 前沿进展的必备列表。

**best-of-algorithmic-trading**
每周自动更新的算法交易开源项目排行榜，追踪 110+ 项目、310k+ 综合 Stars，按热度和类别排序，适合持续跟踪社区动态。

---

## 十二、AI/LLM 金融应用 / AI & LLM in Finance

| 项目 | Stars | 语言 | GitHub |
|------|-------|------|--------|
| **TradingAgents** | ~87k | Python | https://github.com/TauricResearch/TradingAgents |
| **FinGPT** | ~20k | Python | https://github.com/AI4Finance-Foundation/FinGPT |
| **FinRobot** | ~6.4k | Python | https://github.com/AI4Finance-Foundation/FinRobot |
| **FinBERT** | ~2.1k | Python | https://github.com/ProsusAI/finBERT |

### 项目简介

**TradingAgents**
2025 年爆红的多智能体 LLM 金融交易框架。模拟真实交易公司团队协作（基本面研究员、技术研究员、风险管理员、交易员），支持 GPT/Gemini/Claude 等主流模型，是当前金融 AI Agent 领域 Stars 增速最快的项目。

**FinGPT**
AI4Finance 基金会出品的开源金融大语言模型框架，提供数据流、微调流水线和多个下游任务（情绪分析、股价预测、研报生成等），被视为 BloombergGPT 的开源平民替代方案。

**FinRobot**
整合 LLM + 强化学习 + 量化分析的开源金融 AI Agent 平台，覆盖投资研究自动化、算法交易策略设计和风险评估，由 AI4Finance 基金会维护。

**FinBERT**
基于 BERT 在大规模金融文本上预训练的情绪分析模型（Prosus AI 出品），专为财经新闻、研究报告、电话会纪要的情绪打标而设计，是金融 NLP 领域的基础预训练模型之一。

---

## 十三、Alpha 因子研究 / Factor Research

| 项目 | Stars | 语言 | GitHub |
|------|-------|------|--------|
| **alphalens** | ~4.3k | Python | https://github.com/quantopian/alphalens |

### 项目简介

**alphalens**
Quantopian 出品的 Alpha 因子性能分析库。分析预测性因子的信息系数（IC）、IC 衰减、分组收益、换手率等关键指标，生成可视化因子分析报告。是因子研究的必备工具，常与 zipline + pyfolio 配合使用形成完整量化研究工具链。

---

## 十四、高频交易 / High-Frequency Trading

| 项目 | Stars | 语言 | GitHub |
|------|-------|------|--------|
| **hftbacktest** | ~4.2k | Python/Rust | https://github.com/nkaz001/hftbacktest |

### 项目简介

**hftbacktest**
开源高频交易与做市策略回测工具。利用完整 tick 数据（Level 2/3 订单簿 + 逐笔成交）进行精确模拟，考虑限价单队列位置与网络延迟，Rust 核心保证高性能，支持 Binance/Bybit 实盘对接示例。是目前开源 HFT 回测领域最严谨的工具。

---

## 十五、券商 API 接口 / Broker API Clients

| 项目 | Stars | 语言 | GitHub |
|------|-------|------|--------|
| **ib_insync** | ~3.2k | Python | https://github.com/erdewit/ib_insync |

### 项目简介

**ib_insync**
Interactive Brokers TWS/IB Gateway 的 Python 同步/异步封装框架，大幅简化原生 IB API 的复杂度，支持股票、期权、期货、外汇等多资产实盘交易。原作者 2024 年去世后已 archived，社区 fork `ib_async` 延续维护。

---

## 十六、个人财务管理 / Personal Finance

| 项目 | Stars | 语言 | GitHub |
|------|-------|------|--------|
| **Firefly III** | ~23.8k | PHP | https://github.com/firefly-iii/firefly-iii |

### 项目简介

**Firefly III**
最流行的自托管个人财务管理系统。支持收支记录、预算管理、账单跟踪、多账户多货币、数据导入导出，提供完整 REST API，适合搭建私有家庭财务中心或企业内部报销系统。Docker 部署友好。

---

## 十七、财富/投资组合追踪 / Wealth & Portfolio Tracking

| 项目 | Stars | 语言 | GitHub |
|------|-------|------|--------|
| **Ghostfolio** | ~8.6k | TypeScript | https://github.com/ghostfolio/ghostfolio |
| **Wealthfolio** | ~7.7k | Rust/Tauri | https://github.com/wealthfolio/wealthfolio |
| **rotki** | ~3.9k | Python/Vue | https://github.com/rotki/rotki |

### 项目简介

**Ghostfolio**
开源财富管理软件（Angular + NestJS + Prisma 全栈），支持股票、ETF、加密货币多资产追踪，可自托管，提供资产配置分析、持仓概览和投资绩效报告，适合注重数据主权的个人投资者。

**Wealthfolio**
本地优先（Local-First）的桌面投资组合追踪器，Rust + Tauri 实现，数据全部存储于本地无需账户注册，支持 CSV 导入、净值追踪、Monte Carlo 模拟和 FIRE 退休计算。

**rotki**
隐私优先的开源投资组合追踪与加密货币税务报告工具。数据加密存储于本地，支持 100+ CEX/DeFi 协议的交易导入，自动计算各国税务报告，是最流行的本地化加密资产税务工具。

---

## 十八、机构量化工具 / Institutional Quant Tools

| 项目 | Stars | 语言 | GitHub |
|------|-------|------|--------|
| **gs-quant (Goldman Sachs)** | ~10.9k | Python | https://github.com/goldmansachs/gs-quant |

### 项目简介

**gs-quant**
高盛开源的 Python 量化金融工具包，构建于其内部风险转移平台之上。提供衍生品定价、结构化产品分析、期权希腊字母计算和风险管理接口，可对接高盛 Marquee Developer API，是罕见的顶级投行开源量化工具。

---

## 十九、金融数据库 / Financial Databases

| 项目 | Stars | 语言 | GitHub |
|------|-------|------|--------|
| **FinanceDatabase** | ~6.7k | Python | https://github.com/JerBouma/FinanceDatabase |
| **FinanceToolkit** | ~3.5k | Python | https://github.com/JerBouma/FinanceToolkit |
| **arctic (Man Group)** | ~3.1k | Python | https://github.com/man-group/arctic |
| **ArcticDB (Man Group)** | ~2.2k | Python/C++ | https://github.com/man-group/ArcticDB |

### 项目简介

**FinanceDatabase**
包含 30 万+ 证券代码的开源金融数据库，覆盖股票、ETF、基金、指数、货币、加密货币，支持按国家、行业、市值等维度筛选，是构建量化策略选股池和批量数据拉取的实用基础数据源。

**FinanceToolkit**
透明高效的 Python 财务分析库，实现 200+ 财务比率（PE/PB/ROE/Sharpe 等）计算，完整公开所有计算方法避免黑盒依赖，支持股票、ETF、加密等多资产类别，与 FinanceDatabase 同一作者。

**arctic (Man Group)**
Man Group 开源的高性能时序/Tick 数据存储库，基于 MongoDB，支持版本控制（类似 git），专为金融 OHLCV 和 Tick 数据设计。现已进入维护模式，官方推荐迁移至 ArcticDB。

**ArcticDB (Man Group)**
Man Group 与 Bloomberg 联合开发的高性能 DataFrame 数据库。Python-native API，C++ 压缩存储引擎，可将 Pandas DataFrame 直接读写到 S3/LMDB，为金融大规模历史数据分析场景优化设计。

---

## 二十、Java 生态 / Java Ecosystem

| 项目 | Stars | 语言 | GitHub |
|------|-------|------|--------|
| **ta4j** | ~2.4k | Java | https://github.com/ta4j/ta4j |

### 项目简介

**ta4j**
Java 生态中最完整的技术分析库。提供可组合策略 API、30+ 分析准则（Sharpe/最大回撤/Calmar 等）、K 线形态识别和内置回测引擎，原生多线程支持可并行回测数百策略，是 Java/Kotlin 量化开发的首选工具。

---

## 二十一、个人财务与会计 / Personal Finance & Accounting

| 项目 | Stars | 语言 | GitHub |
|------|-------|------|--------|
| **Ledger-CLI** | ~5.9k | C++ | https://github.com/ledger/ledger |
| **hledger** | ~4.5k | Haskell | https://github.com/simonmichael/hledger |
| **beancount** | ~3k | Python | https://github.com/beancount/beancount |
| **paisa** | ~3.1k | Go | https://github.com/ananthakumaran/paisa |
| **fava** | ~2.4k | Python | https://github.com/beancount/fava |

### 项目简介

**Ledger-CLI**
纯文本复式记账（Plain Text Accounting）系统的鼻祖，始于 2003 年。所有数据以纯文本文件存储，无数据库依赖，支持多币种和精确财务报告，是 PTA 生态的基础。

**hledger**
Ledger-CLI 的 Haskell 重写版，更快速健壮，提供 CLI、TUI（终端图形界面）和 Web 三种交互方式，是纯文本记账（Plain Text Accounting）生态中目前最活跃的实现。

**beancount**
以严格数据完整性著称的复式记账工具，V3 版本用 C++ 核心显著提升性能。数据格式比 Ledger 更结构化，适合有复杂财务结构（多账户/多币种/股票持仓）的用户。

**paisa**
基于 Ledger/hledger 构建的开源个人财务管理器（Go 实现），专注印度市场（共同基金、NPS、NSE 股票）的净值追踪，数据全部存储于本地，隐私友好，也适合其他市场用户改造。

**fava**
Beancount 的官方 Web UI，提供图表、账户余额、交易流水、收支报告等可视化界面，使 Beancount 的数据分析体验大幅提升，是 Beancount 用户的必装配套工具。

---

## 二十二、Go 语言生态 / Go Ecosystem

| 项目 | Stars | 语言 | GitHub |
|------|-------|------|--------|
| **ticker** | ~6k | Go | https://github.com/achannarasappa/ticker |
| **GoCryptoTrader** | ~2.2k | Go | https://github.com/thrasher-corp/gocryptotrader |
| **mop** | ~2.2k | Go | https://github.com/mop-tracker/mop |

### 项目简介

**ticker**
终端实时行情追踪工具（Go），支持股票、加密货币价格及持仓盈亏实时显示，YAML 配置自选股和成本基础，数据源支持 Yahoo Finance/Coinbase，是开发者在终端环境中查看行情的利器。

**GoCryptoTrader**
Go 语言实现的加密货币交易机器人框架，支持 30+ 主流交易所（Binance、OKX、Bybit 等），提供完整的回测、模拟交易和实盘交易能力，代码质量高，适合 Go 技术栈团队。

**mop**
专为开发者打造的终端股票行情追踪器（Go），界面简洁，实时显示多只股票价格和涨跌，适合在服务器 / 终端环境快速查看市场数据。

---

## 快速选型参考

| 需求场景 | 推荐项目 |
|---------|---------|
| 投资研究平台（Bloomberg 替代） | OpenBB |
| A 股量化交易（国内接口） | vnpy / rqalpha / WonderTrader |
| 策略回测（Python 生态） | backtrader / backtesting.py / vectorbt |
| 快速策略原型验证 | backtesting.py |
| 大规模参数寻优 | vectorbt |
| 系统化期货交易 | pysystemtrade |
| 生产级高性能交易 | nautilus_trader / Lean |
| 高频/做市策略回测 | hftbacktest |
| 加密货币自动交易 | freqtrade / CCXT + jesse |
| 加密做市/套利 | Hummingbot |
| 加密资产税务报告 | rotki |
| A 股数据获取 | AKShare / tushare |
| 全球股票数据 | yfinance |
| 30 万+ 证券目录 | FinanceDatabase |
| 金融时序大数据存储 | ArcticDB |
| 技术指标计算 | TA-Lib / pandas-ta |
| Java 技术分析 | ta4j |
| Alpha 因子研究 | alphalens |
| 投资组合优化 | PyPortfolioOpt / Riskfolio-Lib |
| 策略绩效分析 | pyfolio / QuantStats |
| 期权/衍生品定价 | QuantLib / gs-quant |
| AI/ML 量化研究 | Qlib / FinRL |
| LLM 多智能体交易 | TradingAgents |
| 金融大模型微调 | FinGPT |
| 金融 NLP/情绪分析 | FinBERT |
| IB 实盘接口 | ib_insync / ib_async |
| Web 交易图表 | lightweight-charts |
| React K 线图 | react-stockcharts |
| Python K 线图 | mplfinance |
| 终端行情追踪 | ticker（Go）|
| Go 加密货币交易 | GoCryptoTrader |
| 个人投资组合追踪 | Ghostfolio / Wealthfolio |
| 个人/家庭记账 | Firefly III / beancount + fava |
| 纯文本记账 | Ledger-CLI / hledger |
