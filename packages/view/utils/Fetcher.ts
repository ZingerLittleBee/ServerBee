// @ts-ignore
const fetcher = (...args: any[]) => fetch(...args).then((res) => res.json())


export default fetcher
