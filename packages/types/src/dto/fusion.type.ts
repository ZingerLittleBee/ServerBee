import { Os, Overview, Process, Realtime, SimpleProcess } from '../fusion.type'

export type FusionDto = {
    overview?: Overview
    os?: Os
    realtime?: Realtime
    simpleProcess?: SimpleProcess[]
    fullProcess?: Process[]
    clientId?: string
    time?: number
}
