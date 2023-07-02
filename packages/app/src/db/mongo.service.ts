import { Injectable } from '@nestjs/common'
import { InjectModel } from '@nestjs/mongoose'
import { Model } from 'mongoose'
import { Fusion } from '@/db/schemas/fusion.schema'
import { FusionDto } from '@/db/dto/fusion.dto'

@Injectable()
export class MongoService {
    constructor(@InjectModel(Fusion.name) private fusionModel: Model<Fusion>) {}

    async createFusion(fusion: FusionDto) {
        const createdFusion = new this.fusionModel(fusion)
        return createdFusion.save()
    }
}
