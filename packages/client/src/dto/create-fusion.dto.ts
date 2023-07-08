import { Os, Overview, Process, Realtime } from '@server-octopus/types'

export class CreateFusionDto {
    overview?: Overview
    os?: Os
    realtime?: Realtime
    full_process?: Process[]
}
