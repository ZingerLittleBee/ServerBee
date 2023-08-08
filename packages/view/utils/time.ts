import dayjs from "dayjs"
import LocalizedFormat from "dayjs/plugin/localizedFormat"

dayjs.extend(LocalizedFormat)
dayjs.locale('zh-cn')

export const formatTime = (time: string | number | Date, format: string = 'YYYY-MM-DD HH:mm:ss') => {
  return dayjs(time).format(format)
}
