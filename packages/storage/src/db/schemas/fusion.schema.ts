import { Prop, Schema, SchemaFactory } from '@nestjs/mongoose'
import { HydratedDocument } from 'mongoose'
import { Os, Overview, Process, Realtime } from '@server-octopus/types'

export type FusionDocument = HydratedDocument<Fusion>

@Schema({
    timestamps: true
})
export class Fusion {
    @Prop({ type: Object })
    overview: Overview

    @Prop({ type: Object })
    os: Os

    @Prop({ type: Object })
    realtime: Realtime

    @Prop({ type: [{ type: Object }] })
    process: Process[]

    @Prop({ type: String })
    clientId: string

    @Prop({ type: Number })
    time: number
}

export const FusionSchema = SchemaFactory.createForClass(Fusion)
