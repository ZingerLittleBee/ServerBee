// Direct re-exports of recharts components.
//
// These were previously wrapped in React.lazy one component at a time, but
// recharts identifies its children (Area, XAxis, Legend, …) by component type
// during chart composition, so per-component lazy wrappers made every chart
// render an empty SVG shell. Chart containers introspect children — the child
// elements must be the real recharts component types.
// biome-ignore lint/performance/noBarrelFile: single shared entry point for recharts so a future (correct) lazy-loading strategy only touches this file
export {
  Area,
  AreaChart,
  Bar,
  BarChart,
  CartesianGrid,
  Legend,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis
} from 'recharts'
