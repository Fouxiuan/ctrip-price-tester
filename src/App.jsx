import { useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import UpdaterDialog, { checkForUpdates } from './UpdaterDialog';

const statusMeta = {
  success: { label: '正常返回', tone: 'success' },
  no_inventory: { label: '暂无可售', tone: 'neutral' },
  restricted: { label: '疑似受限', tone: 'warning' },
  environment_error: { label: '环境异常', tone: 'danger' },
};

function dateAfter(days) {
  const date = new Date();
  date.setHours(12, 0, 0, 0);
  date.setDate(date.getDate() + days);
  return date.toISOString().slice(0, 10);
}

function addDays(value, days) {
  const date = new Date(`${value}T12:00:00`);
  date.setDate(date.getDate() + days);
  return date.toISOString().slice(0, 10);
}

function formatDate(value, options = { month: 'long', day: 'numeric' }) {
  if (!value) return '—';
  return new Intl.DateTimeFormat('zh-CN', options).format(new Date(`${value}T12:00:00`));
}

function formatTime(value) {
  if (!value) return '—';
  return new Intl.DateTimeFormat('zh-CN', {
    month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit',
  }).format(new Date(value));
}

function formatPrice(value) {
  return new Intl.NumberFormat('zh-CN').format(value);
}

function SignalDot({ status }) {
  return <span className={`signal-dot signal-${status}`} aria-hidden="true" />;
}

function LoadingLine({ children }) {
  return <span className="loading-line"><i />{children}</span>;
}

function MonthCalendar({ month, entries, minimum, activeDate, onSelect }) {
  const entryMap = new Map(entries.map((entry) => [entry.date, entry]));
  const [year, monthNumber] = month.split('-').map(Number);
  const firstWeekday = new Date(Date.UTC(year, monthNumber - 1, 1)).getUTCDay();
  const totalDays = new Date(Date.UTC(year, monthNumber, 0)).getUTCDate();
  const cells = [
    ...Array.from({ length: firstWeekday }, (_, index) => ({ blank: true, key: `blank-${index}` })),
    ...Array.from({ length: totalDays }, (_, index) => {
      const day = index + 1;
      const date = `${month}-${String(day).padStart(2, '0')}`;
      return { day, date, entry: entryMap.get(date), key: date };
    }),
  ];

  return (
    <section className="month-card">
      <header>
        <strong>{year}年{monthNumber}月</strong>
        <span>{entries.filter((entry) => entry.price).length} 天有价</span>
      </header>
      <div className="weekday-row" aria-hidden="true">
        {['日', '一', '二', '三', '四', '五', '六'].map((day) => <span key={day}>{day}</span>)}
      </div>
      <div className="calendar-grid">
        {cells.map((cell) => {
          if (cell.blank) return <span className="day-cell is-blank" key={cell.key} />;
          const isMinimum = cell.entry?.price === minimum;
          const isActive = activeDate === cell.date;
          return (
            <button
              type="button"
              key={cell.key}
              className={`day-cell ${cell.entry ? 'has-data' : 'is-outside'} ${isMinimum ? 'is-minimum' : ''} ${isActive ? 'is-active' : ''}`}
              disabled={!cell.entry}
              onClick={() => cell.entry && onSelect(cell.entry)}
              aria-label={`${formatDate(cell.date)}${cell.entry?.price ? `，${cell.entry.price}元${isMinimum ? '，最低价' : ''}` : '，暂无日历数据'}`}
            >
              <span className="day-number">{cell.day}</span>
              {cell.entry?.price ? <strong>¥{formatPrice(cell.entry.price)}</strong> : cell.entry ? <small>暂无</small> : null}
              {isMinimum && <em>最低</em>}
            </button>
          );
        })}
      </div>
    </section>
  );
}

function RoomChip({ room, isCheapest }) {
  const isFreeCancel = !String(room.cancelPolicy || '').includes('不可取消');
  const hasCancel = !!room.cancelPolicy;
  const hasMeal = room.meal && room.meal !== '无早餐';
  return (
    <div className={`room-chip ${isCheapest ? 'is-cheapest' : ''}`} title={[room.cancelPolicy, room.bedType, room.area].filter(Boolean).join(' · ') || room.roomName}>
      {isCheapest && <span className="chip-flag">最低价</span>}
      <div className="chip-price-row">
        <span className="chip-room-name">{room.roomName}</span>
        <strong className="chip-price">¥{formatPrice(room.price)}</strong>
      </div>
      <div className="chip-tags">
        {hasCancel && <span className={`chip-tag ${isFreeCancel ? 'tag-free' : 'tag-nonfree'}`}>{isFreeCancel ? '可取消' : '不可取消'}</span>}
        {hasMeal && <span className="chip-tag tag-meal">{room.meal}</span>}
      </div>
    </div>
  );
}

function RoomResult({ result, loading, date }) {
  if (loading) {
    return (
      <section className="room-result is-loading" aria-live="polite">
        <LoadingLine>正在核对 {formatDate(date)} 的房型与价格…</LoadingLine>
        <p>后台会访问一次携程房价接口，通常需要 8–30 秒。</p>
      </section>
    );
  }
  if (!result) return null;
  const meta = statusMeta[result.status] || statusMeta.environment_error;
  const rooms = [...result.rooms].sort((a, b) => a.price - b.price);
  const cheapest = rooms[0];
  const otherRooms = rooms.slice(1);
  return (
    <section className="room-result" aria-live="polite">
      <div className="room-result-heading">
        <div>
          <span>{formatDate(date, { year: 'numeric', month: 'long', day: 'numeric', weekday: 'short' })}</span>
          <h2>当日房型核验</h2>
        </div>
        <span className={`status-badge tone-${meta.tone}`}>{meta.label}</span>
      </div>
      <p className="result-message">{result.message}</p>
      <div className="evidence-strip">
        {result.signals.map((signal, index) => (
          <div key={`${signal.label}-${index}`}>
            <SignalDot status={signal.status} />
            <span><strong>{signal.label}</strong><small>{signal.detail}</small></span>
          </div>
        ))}
      </div>
      {rooms.length > 0 && (
        <div className="room-cards">
          {cheapest && <RoomChip room={cheapest} isCheapest />}
          {otherRooms.length > 0 && (
            <div className="room-chip-row">
              {otherRooms.map((room, index) => (
                <RoomChip key={`${room.saleRoomId}-${index}`} room={room} />
              ))}
            </div>
          )}
        </div>
      )}
      <p className="interpretation-note">“疑似受限”表示页面或接口未按预期返回，不能单独证明账号已封。可用同一酒店和日期换一台正常机器复测，排除无房、网络及浏览器扩展问题。</p>
    </section>
  );
}

export default function App() {
  const calendarSectionRef = useRef(null);
  const [query, setQuery] = useState('');
  const [searching, setSearching] = useState(false);
  const [savedHotels, setSavedHotels] = useState([]);
  const [lastSearchEmpty, setLastSearchEmpty] = useState(false);
  const [searchDuration, setSearchDuration] = useState(null);
  const [searchTouched, setSearchTouched] = useState(false);
  const [selectedHotel, setSelectedHotel] = useState(null);
  const [calendar, setCalendar] = useState(null);
  const [calendarLoading, setCalendarLoading] = useState(false);
  const [activeDate, setActiveDate] = useState('');
  const [roomResult, setRoomResult] = useState(null);
  const [roomLoading, setRoomLoading] = useState(false);
  const [error, setError] = useState('');
  const [environment, setEnvironment] = useState({ ready: false, checking: true, message: '正在检查 OpenCLI 与浏览器连接…' });
  const [pendingUpdate, setPendingUpdate] = useState(null);

  const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

  const checkEnvironment = async () => {
    if (!isTauri) {
      setEnvironment({ ready: false, checking: false, message: '网页预览不能执行查价，请启动桌面版。' });
      return;
    }
    setEnvironment((previous) => ({ ...previous, checking: true }));
    try {
      const data = await invoke('check_environment');
      setEnvironment({ ...data, checking: false });
    } catch (reason) {
      setEnvironment({ ready: false, checking: false, message: String(reason) });
    }
  };

  const refreshSavedHotels = async () => {
    try {
      setSavedHotels(await invoke('load_searched_hotels'));
    } catch {
      // 数据库不可用时不打断搜索主流程
    }
  };

  useEffect(() => {
    checkEnvironment();
    if (isTauri) {
      refreshSavedHotels();
      const timer = window.setTimeout(() => {
        checkForUpdates(setPendingUpdate);
      }, 3000);
      return () => window.clearTimeout(timer);
    }
  }, []);

  useEffect(() => {
    if (!selectedHotel) return undefined;
    const frame = window.requestAnimationFrame(() => {
      calendarSectionRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    });
    return () => window.cancelAnimationFrame(frame);
  }, [selectedHotel?.hotelId]);

  const loadCalendar = async (hotel) => {
    setSelectedHotel(hotel);
    setCalendar(null);
    setCalendarLoading(true);
    setRoomResult(null);
    setActiveDate('');
    setError('');
    try {
      const data = await invoke('get_ctrip_price_calendar', {
        request: { hotelId: hotel.hotelId, startDate: dateAfter(1) },
      });
      setCalendar(data);
      if (data.minPrice) {
        invoke('update_hotel_min_price', { hotelId: hotel.hotelId, minPrice: data.minPrice }).catch(() => {});
        refreshSavedHotels();
      }
      setSelectedHotel((current) => ({
        ...current,
        hotelId: data.resolvedHotelId || current.hotelId,
        name: data.hotelName || current.name,
      }));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setCalendarLoading(false);
    }
  };

  const handleSearch = async (event) => {
    event.preventDefault();
    if (!query.trim() || searching) return;
    const started = performance.now();
    setSearching(true);
    setSearchTouched(true);
    setSearchDuration(null);
    setCalendar(null);
    setRoomResult(null);
    setError('');
    try {
      const results = await invoke('search_ctrip_hotels', { request: { query: query.trim() } });
      setLastSearchEmpty(results.length === 0);
      await refreshSavedHotels();
      if (results.length === 1) void loadCalendar(results[0]);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setSearching(false);
      setSearchDuration(Number(((performance.now() - started) / 1000).toFixed(1)));
    }
  };

  const runDayTest = async (entry) => {
    if (!selectedHotel || roomLoading) return;
    setActiveDate(entry.date);
    setRoomLoading(true);
    setRoomResult(null);
    setError('');
    try {
      const data = await invoke('test_ctrip_price', {
        request: {
          hotelId: selectedHotel.hotelId,
          checkin: entry.date,
          checkout: addDays(entry.date, 1),
        },
      });
      setRoomResult(data);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setRoomLoading(false);
    }
  };

  const months = useMemo(() => {
    const grouped = new Map();
    for (const entry of calendar?.prices || []) {
      const key = entry.date.slice(0, 7);
      if (!grouped.has(key)) grouped.set(key, []);
      grouped.get(key).push(entry);
    }
    return [...grouped.entries()];
  }, [calendar]);

  const pricedDays = calendar?.prices.filter((entry) => entry.price).length || 0;
  const calendarMeta = calendar ? (statusMeta[calendar.status] || statusMeta.environment_error) : null;

  return (
    <main className="app-shell">
      <header className="topbar">
        <div className="brand-mark" aria-hidden="true"><span>C</span><i /></div>
        <div className="brand-copy"><strong>携程查价测试台</strong><span>酒店价格日历 · 独立风控验证</span></div>
        <button type="button" className={`environment-pill ${environment.ready ? 'is-ready' : ''}`} onClick={checkEnvironment} disabled={environment.checking} title={environment.message}>
          <SignalDot status={environment.checking ? 'running' : environment.ready ? 'success' : 'error'} />
          {environment.checking ? '环境检测中' : environment.ready ? '环境可用' : '环境待处理'}
        </button>
      </header>

      <section className="search-stage">
        <div className="search-copy">
          <span className="eyebrow">HOTEL PRICE CALENDAR</span>
          <p>支持酒店名称、关键词、携程酒店 ID 或详情页链接。日历一次读取完整价格区间，减少重复请求。</p>
        </div>
        <form className="hotel-search" onSubmit={handleSearch}>
          <label htmlFor="hotel-query">搜索酒店</label>
          <div className="search-control">
            <span className="search-icon" aria-hidden="true" />
            <input
              id="hotel-query"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="输入酒店名称、关键词或酒店 ID，例如：上海和平饭店"
              autoFocus
            />
            <button type="submit" disabled={!query.trim() || searching}>
              {searching ? <LoadingLine>搜索中</LoadingLine> : '搜索酒店'}
            </button>
          </div>
          <small>直接输入数字 ID 时会跳过搜索结果，立即读取该酒店价格日历。</small>
        </form>
      </section>

      {error && <div className="error-callout" role="alert"><strong>操作未完成</strong><span>{error}</span></div>}

      {savedHotels.length > 0 && (
        <section className="saved-hotels">
          <header>
            <div><span>已搜索酒店</span><strong>点击卡片查看最低价与价格日历</strong></div>
            <small>{savedHotels.length} 家{searchDuration ? ` · 最近搜索用时 ${searchDuration} 秒` : ''}</small>
          </header>
          <div className="hotel-cards">
            {savedHotels.map((hotel) => (
              <button
                type="button"
                key={hotel.hotelId}
                className={hotel.hotelId === selectedHotel?.hotelId ? 'is-active' : ''}
                onClick={() => loadCalendar(hotel)}
              >
                <span className="hotel-pin" aria-hidden="true" />
                <span className="hotel-card-body">
                  <strong>{hotel.name}</strong>
                  <small>{[hotel.cityName, hotel.provinceName].filter(Boolean).join(' · ') || '地点由详情页确认'}</small>
                </span>
                {hotel.minPrice ? (
                  <span className="hotel-card-price"><sup>¥</sup>{formatPrice(hotel.minPrice)}<small>最低价</small></span>
                ) : (
                  <span className="hotel-card-cta">查看价格 <i aria-hidden="true">→</i></span>
                )}
              </button>
            ))}
          </div>
        </section>
      )}

      {searchTouched && !searching && lastSearchEmpty && !error && (
        <section className="empty-search"><strong>没有匹配的酒店</strong><p>试试完整酒店名称，或直接粘贴携程酒店 ID / 详情页链接。</p></section>
      )}

      {(selectedHotel || calendarLoading) && (
        <section className="calendar-workspace" ref={calendarSectionRef}>
          <header className="hotel-heading">
            <button type="button" className="back-button" onClick={() => { setSelectedHotel(null); setCalendar(null); setRoomResult(null); }}>← 返回列表</button>
            <div>
              <span>{selectedHotel?.cityName || '携程酒店'}</span>
              <h2>{selectedHotel?.name || '正在确认酒店信息…'}</h2>
              <small>酒店 ID {selectedHotel?.hotelId}</small>
            </div>
            {calendar && <span className={`status-badge tone-${calendarMeta.tone}`}>{calendarMeta.label}</span>}
          </header>

          {calendarLoading && (
            <div className="calendar-loading">
              <div className="calendar-skeleton" aria-hidden="true">{Array.from({ length: 28 }, (_, index) => <i key={index} />)}</div>
              <LoadingLine>正在读取酒店价格日历，通常需要 8–30 秒…</LoadingLine>
            </div>
          )}

          {!calendarLoading && calendar && (
            <>
              <div className="calendar-layout">
                <div className="calendar-main">
                  <div className="calendar-toolbar">
                    <div><strong>全部日期价格</strong><span>点击任一日期，可继续核验当天房型</span></div>
                    <div className="legend"><span><i className="legend-min" />最低价</span><span><i className="legend-price" />有价格</span><span><i className="legend-empty" />暂无</span></div>
                  </div>
                  <div className="months-grid">
                    {months.map(([month, entries]) => (
                      <MonthCalendar key={month} month={month} entries={entries} minimum={calendar.minPrice} activeDate={activeDate} onSelect={runDayTest} />
                    ))}
                  </div>
                </div>

                <aside className="lowest-panel">
                  <span className="panel-kicker">LOWEST FOUND</span>
                  {calendar.minPrice ? (
                    <>
                      <p className="lowest-price"><sup>¥</sup>{formatPrice(calendar.minPrice)}</p>
                      <strong>当前日历最低价</strong>
                      <div className="lowest-dates">
                        {calendar.minDates.slice(0, 6).map((date) => <button type="button" key={date} onClick={() => runDayTest({ date, price: calendar.minPrice })}>{formatDate(date, { month: 'short', day: 'numeric', weekday: 'short' })}<span>查房型 →</span></button>)}
                        {calendar.minDates.length > 6 && <small>另有 {calendar.minDates.length - 6} 天同价</small>}
                      </div>
                    </>
                  ) : <><p className="lowest-price is-empty">—</p><strong>当前区间暂无可售价格</strong></>}
                  <dl>
                    <div><dt>日历范围</dt><dd>{calendar.prices.length} 天</dd></div>
                    <div><dt>有价日期</dt><dd>{pricedDays} 天</dd></div>
                    <div><dt>读取耗时</dt><dd>{(calendar.durationMs / 1000).toFixed(1)} 秒</dd></div>
                    <div><dt>更新时间</dt><dd>{formatTime(calendar.startedAt)}</dd></div>
                  </dl>
                  <p>{calendar.message}</p>
                </aside>
              </div>

              <RoomResult result={roomResult} loading={roomLoading} date={activeDate} />
            </>
          )}
        </section>
      )}

      {!searchTouched && !selectedHotel && (
        <footer className="idle-guide">
          <span>名称 / 关键词</span><i />
          <span>选择酒店</span><i />
          <span>查看全日历最低价</span><i />
          <span>按日期核验房型</span>
        </footer>
      )}

      {pendingUpdate && <UpdaterDialog update={pendingUpdate} onClose={() => setPendingUpdate(null)} />}
    </main>
  );
}
