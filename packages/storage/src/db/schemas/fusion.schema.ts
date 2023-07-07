import { Prop, Schema, SchemaFactory } from '@nestjs/mongoose'
import { HydratedDocument } from 'mongoose'
import {
    Os,
    Overview,
    Process,
    Realtime,
    SimpleProcess
} from '@server-octopus/types'

type FusionDocument = HydratedDocument<FusionModel>

@Schema({
    timestamps: true
})
class FusionModel {
    @Prop({ type: Object })
    overview: Overview

    @Prop({ type: Object })
    os: Os

    @Prop({ type: Object })
    realtime: Realtime

    @Prop({ type: [{ type: Object }] })
    simpleProcess: SimpleProcess[]

    @Prop({ type: [{ type: Object }] })
    fullProcess: Process[]

    @Prop({ type: String })
    clientId: string

    @Prop({ type: Number })
    time: number
}

const FusionSchema = SchemaFactory.createForClass(FusionModel)

export { FusionSchema, FusionDocument, FusionModel }
